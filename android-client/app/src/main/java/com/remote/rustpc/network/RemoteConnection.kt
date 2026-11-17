package com.remote.rustpc.network

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.util.Log
import com.remote.rustpc.data.*
import kotlinx.coroutines.*
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import java.io.BufferedInputStream
import java.io.BufferedOutputStream
import java.net.InetSocketAddress
import java.net.Socket

sealed class ConnectionState {
    data object Disconnected : ConnectionState()
    data object Connecting : ConnectionState()
    data object Authenticating : ConnectionState()
    data class Connected(val screenWidth: Int, val screenHeight: Int) : ConnectionState()
    data class Error(val message: String) : ConnectionState()
}

data class ConnectionStats(
    val fps: Int = 0,
    val rttMs: Long = 0L,
    val screenWidth: Int = 0,
    val screenHeight: Int = 0,
    val frameCount: Long = 0L
)

class RemoteConnection(private val coroutineScope: CoroutineScope) {

    private val tag = "RemoteConnection"

    private val _connectionState = MutableStateFlow<ConnectionState>(ConnectionState.Disconnected)
    val connectionState: StateFlow<ConnectionState> = _connectionState.asStateFlow()

    private val _latestBitmap = MutableStateFlow<Bitmap?>(null)
    val latestBitmap: StateFlow<Bitmap?> = _latestBitmap.asStateFlow()

    private val _stats = MutableStateFlow(ConnectionStats())
    val stats: StateFlow<ConnectionStats> = _stats.asStateFlow()

    private var socket: Socket? = null
    private var connectionJob: Job? = null
    private var pingJob: Job? = null
    private var writeJob: Job? = null

    // Channel for thread-safe sequential packet writing
    private var outgoingChannel = Channel<Packet>(Channel.UNLIMITED)

    // FPS calculation state
    private var frameCountInWindow = 0
    private var lastFpsTimestamp = System.currentTimeMillis()
    private var totalFrames = 0L

    fun connect(host: String, port: Int, password: String) {
        if (_connectionState.value is ConnectionState.Connecting || _connectionState.value is ConnectionState.Connected) {
            return
        }

        disconnect()

        outgoingChannel = Channel(Channel.UNLIMITED)

        connectionJob = coroutineScope.launch(Dispatchers.IO) {
            try {
                Log.d(tag, "Connecting to $host:$port...")
                _connectionState.value = ConnectionState.Connecting

                val sock = Socket()
                sock.tcpNoDelay = true
                sock.soTimeout = 15000 // 15s handshake timeout
                sock.connect(InetSocketAddress(host, port), 8000)

                socket = sock
                val out = BufferedOutputStream(sock.getOutputStream(), 64 * 1024)
                val inp = BufferedInputStream(sock.getInputStream(), 256 * 1024)

                // 1. Send AuthRequest
                _connectionState.value = ConnectionState.Authenticating
                val tokenHash = ProtocolCodec.hashPassword(password)
                val authPacket = Packet.AuthRequest(
                    clientVersion = PROTOCOL_VERSION,
                    tokenHash = tokenHash
                )
                val authBytes = ProtocolCodec.encodePacket(authPacket)
                out.write(authBytes)
                out.flush()

                // 2. Read AuthResponse
                val responsePacket = ProtocolCodec.readPacket(inp)
                if (responsePacket !is Packet.AuthResponse) {
                    throw IllegalStateException("Expected AuthResponse from host")
                }

                if (!responsePacket.success) {
                    throw IllegalStateException(responsePacket.message.ifBlank { "Authentication failed" })
                }

                Log.d(tag, "Authenticated! Remote screen: ${responsePacket.screenWidth}x${responsePacket.screenHeight}")

                _connectionState.value = ConnectionState.Connected(
                    responsePacket.screenWidth,
                    responsePacket.screenHeight
                )
                _stats.value = _stats.value.copy(
                    screenWidth = responsePacket.screenWidth,
                    screenHeight = responsePacket.screenHeight
                )

                // Start dedicated packet writer coroutine
                writeJob = launch(Dispatchers.IO) {
                    try {
                        for (packet in outgoingChannel) {
                            val bytes = ProtocolCodec.encodePacket(packet)
                            out.write(bytes)
                            out.flush()
                        }
                    } catch (e: Exception) {
                        Log.e(tag, "Write error: ${e.message}")
                    }
                }

                // Start ping / keepalive job
                startPingJob()

                // Active streaming read loop
                sock.soTimeout = 0 // Infinite read timeout
                val decodeOptions = BitmapFactory.Options().apply {
                    inPreferredConfig = Bitmap.Config.ARGB_8888
                }

                while (isActive && !sock.isClosed) {
                    val packet = ProtocolCodec.readPacket(inp)
                    when (packet) {
                        is Packet.VideoFrame -> {
                            handleVideoFrame(packet, decodeOptions)
                        }
                        is Packet.Pong -> {
                            val rtt = System.currentTimeMillis() - packet.timestampMs
                            _stats.value = _stats.value.copy(rttMs = rtt.coerceAtLeast(0))
                        }
                        is Packet.Ping -> {
                            sendPacket(Packet.Pong(packet.timestampMs))
                        }
                        else -> {
                            // Other packets ignored
                        }
                    }
                }

            } catch (e: Exception) {
                Log.e(tag, "Connection error: ${e.message}", e)
                if (isActive) {
                    _connectionState.value = ConnectionState.Error(e.message ?: "Connection error")
                }
            } finally {
                cleanupSocket()
            }
        }
    }

    private fun handleVideoFrame(frame: Packet.VideoFrame, options: BitmapFactory.Options) {
        val bitmap = try {
            BitmapFactory.decodeByteArray(frame.data, 0, frame.data.size, options)
        } catch (e: Exception) {
            null
        }

        if (bitmap != null) {
            _latestBitmap.value = bitmap
            frameCountInWindow++
            totalFrames++

            val now = System.currentTimeMillis()
            val elapsed = now - lastFpsTimestamp
            if (elapsed >= 1000) {
                val currentFps = ((frameCountInWindow * 1000f) / elapsed).toInt()
                _stats.value = _stats.value.copy(
                    fps = currentFps,
                    frameCount = totalFrames
                )
                frameCountInWindow = 0
                lastFpsTimestamp = now
            }
        }
    }

    private fun startPingJob() {
        pingJob?.cancel()
        pingJob = coroutineScope.launch(Dispatchers.IO) {
            while (isActive) {
                delay(2000)
                sendPacket(Packet.Ping(System.currentTimeMillis()))
            }
        }
    }

    fun sendMouseMove(xRatio: Float, yRatio: Float) {
        val clampedX = xRatio.coerceIn(0f, 1f)
        val clampedY = yRatio.coerceIn(0f, 1f)
        sendPacket(Packet.Input(InputEvent.MouseMove(clampedX, clampedY)))
    }

    fun sendMouseDown(button: MouseBtn) {
        sendPacket(Packet.Input(InputEvent.MouseDown(button)))
    }

    fun sendMouseUp(button: MouseBtn) {
        sendPacket(Packet.Input(InputEvent.MouseUp(button)))
    }

    fun sendMouseClick(button: MouseBtn) {
        sendMouseDown(button)
        sendMouseUp(button)
    }

    fun sendMouseDoubleClick(button: MouseBtn) {
        sendMouseDown(button)
        sendMouseUp(button)
        sendMouseDown(button)
        sendMouseUp(button)
    }

    fun sendMouseScroll(deltaX: Int, deltaY: Int) {
        sendPacket(Packet.Input(InputEvent.MouseScroll(deltaX, deltaY)))
    }

    fun sendKeyDown(key: KeyCode) {
        sendPacket(Packet.Input(InputEvent.KeyDown(key)))
    }

    fun sendKeyUp(key: KeyCode) {
        sendPacket(Packet.Input(InputEvent.KeyUp(key)))
    }

    fun sendKeyClick(key: KeyCode) {
        sendKeyDown(key)
        sendKeyUp(key)
    }

    fun sendText(text: String) {
        for (char in text) {
            sendKeyClick(KeyCode.Unicode(char))
        }
    }

    fun sendClipboard(text: String) {
        sendPacket(Packet.Clipboard(text))
    }

    fun sendPacket(packet: Packet) {
        outgoingChannel.trySend(packet)
    }

    fun disconnect() {
        pingJob?.cancel()
        writeJob?.cancel()
        connectionJob?.cancel()
        outgoingChannel.close()
        cleanupSocket()
        _connectionState.value = ConnectionState.Disconnected
        _latestBitmap.value = null
        _stats.value = ConnectionStats()
    }

    private fun cleanupSocket() {
        try {
            socket?.close()
        } catch (_: Exception) {}
        socket = null
    }
}

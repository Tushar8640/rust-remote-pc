package com.remote.rustpc.network

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.util.Log
import com.remote.rustpc.data.*
import kotlinx.coroutines.*
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import java.io.BufferedInputStream
import java.io.BufferedOutputStream
import java.net.ConnectException
import java.net.InetSocketAddress
import java.net.NoRouteToHostException
import java.net.Socket
import java.net.SocketTimeoutException
import java.net.UnknownHostException

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

    // Audio state (UI observes these)
    private val _audioMuted = MutableStateFlow(false)
    val audioMuted: StateFlow<Boolean> = _audioMuted.asStateFlow()

    private val _audioVolume = MutableStateFlow(1f)
    val audioVolume: StateFlow<Float> = _audioVolume.asStateFlow()

    /** True once the host has started sending audio packets. */
    private val _audioActive = MutableStateFlow(false)
    val audioActive: StateFlow<Boolean> = _audioActive.asStateFlow()

    // Quality mode requested from the host (the UI observes this)
    private val _qualityMode = MutableStateFlow(QualityMode.AUTO)
    val qualityMode: StateFlow<QualityMode> = _qualityMode.asStateFlow()

    private val audioPlayer = AudioPlayer()

    private var socket: Socket? = null
    private var connectionJob: Job? = null

    // Channel for thread-safe sequential packet writing
    private var outgoingChannel = Channel<Packet>(Channel.UNLIMITED)

    // FPS calculation state (touched only by the decoder coroutine)
    private var frameCountInWindow = 0
    private var lastFpsTimestamp = System.currentTimeMillis()
    private var totalFrames = 0L

    fun connect(
        host: String,
        port: Int,
        password: String,
        quality: QualityMode = QualityMode.AUTO
    ) {
        val st = _connectionState.value
        if (st is ConnectionState.Connecting || st is ConnectionState.Authenticating || st is ConnectionState.Connected) {
            return
        }

        disconnect()

        outgoingChannel = Channel(Channel.UNLIMITED)
        frameCountInWindow = 0
        totalFrames = 0L
        lastFpsTimestamp = System.currentTimeMillis()
        _audioActive.value = false
        audioPlayer.setVolume(_audioVolume.value)
        audioPlayer.setMuted(_audioMuted.value)

        connectionJob = coroutineScope.launch(Dispatchers.IO) {
            try {
                _connectionState.value = ConnectionState.Connecting

                // A 64-character Pairing ID means "reach the PC over the internet":
                // the native bridge dials it peer-to-peer and hands back a loopback
                // port (+ a one-time secret) that speaks the same protocol as TCP.
                var connectHost = host
                var connectPort = port
                var preamble: ByteArray? = null
                val join = if (IrohBridge.isJoinCode(host)) {
                    JoinCode.decode(host) ?: error("That join code is not valid. Copy it again from the PC.")
                } else null
                if (join != null || IrohBridge.isPairingId(host)) {
                    Log.d(tag, "Connecting to ${IrohBridge.shorten(host)} over P2P...")
                    val ep = if (join != null) {
                        IrohBridge.connect(join.pairingId, join.relayUrl, join.pass)
                    } else {
                        IrohBridge.connect(host)
                    }
                    connectHost = "127.0.0.1"
                    connectPort = ep.port
                    preamble = ep.secret.toByteArray(Charsets.US_ASCII)
                } else {
                    Log.d(tag, "Connecting to $host:$port...")
                }

                val sock = Socket()
                sock.tcpNoDelay = true
                sock.soTimeout = 15000 // 15s handshake timeout
                sock.connect(InetSocketAddress(connectHost, connectPort), 8000)

                socket = sock
                val out = BufferedOutputStream(sock.getOutputStream(), 64 * 1024)
                val inp = BufferedInputStream(sock.getInputStream(), 256 * 1024)
                if (preamble != null) {
                    out.write(preamble)
                    out.flush()
                }

                // 1. Send AuthRequest
                _connectionState.value = ConnectionState.Authenticating
                val authPacket = Packet.AuthRequest(
                    clientVersion = PROTOCOL_VERSION,
                    tokenHash = ProtocolCodec.hashPassword(password)
                )
                out.write(ProtocolCodec.encodePacket(authPacket))
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

                _stats.value = ConnectionStats(
                    screenWidth = responsePacket.screenWidth,
                    screenHeight = responsePacket.screenHeight
                )
                _connectionState.value = ConnectionState.Connected(
                    responsePacket.screenWidth,
                    responsePacket.screenHeight
                )

                // Ask the host for the chosen quality mode before video starts flowing.
                _qualityMode.value = quality
                if (quality != QualityMode.AUTO) sendPacket(Packet.SetQuality(quality.id))

                // Packet writer
                val writerChannel = outgoingChannel
                launch(Dispatchers.IO) {
                    try {
                        for (packet in writerChannel) {
                            out.write(ProtocolCodec.encodePacket(packet))
                            out.flush()
                        }
                    } catch (e: Exception) {
                        Log.e(tag, "Write error: ${e.message}")
                    }
                }

                // Keepalive / RTT
                launch(Dispatchers.IO) {
                    while (isActive) {
                        delay(2000)
                        sendPacket(Packet.Ping(System.currentTimeMillis()))
                    }
                }

                // Video decoding runs off the network thread. CONFLATED keeps only the
                // newest undecoded frame, so a slow phone drops frames instead of building latency.
                val videoChannel = Channel<Packet.VideoFrame>(Channel.CONFLATED)
                launch(Dispatchers.Default) {
                    val options = BitmapFactory.Options().apply {
                        inPreferredConfig = Bitmap.Config.ARGB_8888
                    }
                    for (frame in videoChannel) {
                        handleVideoFrame(frame, options)
                    }
                }

                // Audio playback also runs off the network thread (AudioTrack.write blocks).
                // Small buffer, oldest dropped first: bounds audio delay to ~200 ms.
                val audioChannel = Channel<Packet.Audio>(10, BufferOverflow.DROP_OLDEST)
                launch(Dispatchers.IO) {
                    for (a in audioChannel) {
                        audioPlayer.write(a.sampleRate, a.channels, a.data)
                    }
                }

                // Streaming read loop
                sock.soTimeout = 0
                while (isActive && !sock.isClosed) {
                    when (val packet = ProtocolCodec.readPacket(inp)) {
                        is Packet.VideoFrame -> videoChannel.trySend(packet)
                        is Packet.Audio -> {
                            _audioActive.value = true
                            audioChannel.trySend(packet)
                        }
                        is Packet.Pong -> {
                            val rtt = System.currentTimeMillis() - packet.timestampMs
                            _stats.update { it.copy(rttMs = rtt.coerceAtLeast(0)) }
                        }
                        is Packet.Ping -> sendPacket(Packet.Pong(packet.timestampMs))
                        else -> Unit
                    }
                }
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                Log.e(tag, "Connection error: ${e.message}", e)
                if (isActive) {
                    _connectionState.value = ConnectionState.Error(friendlyError(e))
                }
            } finally {
                cleanupSocket()
                audioPlayer.release()
                // The writer/ping/decoder children must not outlive the connection.
                coroutineContext.cancelChildren()
            }
        }
    }

    private fun friendlyError(e: Exception): String = when {
        e is ConnectException -> "Connection refused. Check that sharing is on at the PC and the port is correct."
        e is SocketTimeoutException -> "Timed out. Check the Pairing ID or address, and that sharing is on at the PC."
        e is UnknownHostException -> "Unknown host. Check the address."
        e is NoRouteToHostException -> "No route to the PC. Are both devices on the same network or Tailscale?"
        e.message?.contains("Unexpected end of stream") == true -> "Connection lost. The PC stopped sharing."
        else -> e.message ?: "Connection error"
    }

    private fun handleVideoFrame(frame: Packet.VideoFrame, options: BitmapFactory.Options) {
        val bitmap = try {
            BitmapFactory.decodeByteArray(frame.data, 0, frame.data.size, options)
        } catch (e: Exception) {
            null
        } ?: return

        _latestBitmap.value = bitmap
        frameCountInWindow++
        totalFrames++

        val now = System.currentTimeMillis()
        val elapsed = now - lastFpsTimestamp
        if (elapsed >= 1000) {
            val currentFps = ((frameCountInWindow * 1000f) / elapsed).toInt()
            _stats.update { it.copy(fps = currentFps, frameCount = totalFrames) }
            frameCountInWindow = 0
            lastFpsTimestamp = now
        }
    }

    // ── Audio controls ──

    fun setMuted(muted: Boolean) {
        _audioMuted.value = muted
        audioPlayer.setMuted(muted)
    }

    fun setVolume(volume: Float) {
        val v = volume.coerceIn(0f, 1f)
        _audioVolume.value = v
        audioPlayer.setVolume(v)
        if (v > 0f && _audioMuted.value) setMuted(false)
    }

    // ── Input ──

    /** Changes the video quality mode for the current session. */
    fun setQuality(mode: QualityMode) {
        _qualityMode.value = mode
        sendPacket(Packet.SetQuality(mode.id))
    }

    fun sendMouseMove(xRatio: Float, yRatio: Float) {
        sendPacket(Packet.Input(InputEvent.MouseMove(xRatio.coerceIn(0f, 1f), yRatio.coerceIn(0f, 1f))))
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
        sendMouseClick(button)
        sendMouseClick(button)
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

    /** Presses Ctrl + the given character (e.g. Ctrl+C). */
    fun sendCtrlShortcut(ch: Char) {
        sendKeyDown(KeyCode.Control)
        sendKeyClick(KeyCode.Unicode(ch.code))
        sendKeyUp(KeyCode.Control)
    }

    fun sendText(text: String) {
        var i = 0
        while (i < text.length) {
            val cp = text.codePointAt(i)
            i += Character.charCount(cp)
            when (cp) {
                '\n'.code -> sendKeyClick(KeyCode.Enter)
                '\t'.code -> sendKeyClick(KeyCode.Tab)
                '\r'.code -> Unit
                else -> sendKeyClick(KeyCode.Unicode(cp))
            }
        }
    }

    fun sendClipboard(text: String) {
        sendPacket(Packet.Clipboard(text))
    }

    fun sendPacket(packet: Packet) {
        outgoingChannel.trySend(packet)
    }

    fun disconnect() {
        connectionJob?.cancel()
        connectionJob = null
        outgoingChannel.close()
        cleanupSocket()
        audioPlayer.release()
        _connectionState.value = ConnectionState.Disconnected
        _latestBitmap.value = null
        _stats.value = ConnectionStats()
        _audioActive.value = false
    }

    private fun cleanupSocket() {
        try {
            socket?.close()
        } catch (_: Exception) {
        }
        socket = null
    }
}

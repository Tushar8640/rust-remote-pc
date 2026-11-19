package com.remote.rustpc.network

import com.remote.rustpc.data.*
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.io.DataInputStream
import java.io.DataOutputStream
import java.io.InputStream
import java.io.OutputStream
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.security.MessageDigest

object ProtocolCodec {

    /**
     * Compute SHA-256 hash string for password authentication matching Rust host
     */
    fun hashPassword(password: String): String {
        val digest = MessageDigest.getInstance("SHA-256")
        val hash = digest.digest(password.toByteArray(Charsets.UTF_8))
        return hash.joinToString("") { "%02x".format(it) }
    }

    /**
     * Encodes a Packet into a frame with 4-byte Big-Endian length prefix
     */
    fun encodePacket(packet: Packet): ByteArray {
        val payloadStream = ByteArrayOutputStream()
        writePacketPayload(payloadStream, packet)
        val payload = payloadStream.toByteArray()

        val frameStream = ByteArrayOutputStream(4 + payload.size)
        val dataOut = DataOutputStream(frameStream)
        // 4 bytes Big-Endian length
        dataOut.writeInt(payload.size)
        dataOut.write(payload)
        dataOut.flush()
        return frameStream.toByteArray()
    }

    private fun writePacketPayload(out: ByteArrayOutputStream, packet: Packet) {
        // Rust enum variant is u32 LE (4 bytes)
        writeU32LE(out, packet.variantIndex)

        when (packet) {
            is Packet.AuthRequest -> {
                writeString(out, packet.clientVersion)
                writeString(out, packet.tokenHash)
            }
            is Packet.AuthResponse -> {
                out.write(if (packet.success) 1 else 0)
                writeString(out, packet.message)
                writeU32LE(out, packet.screenWidth)
                writeU32LE(out, packet.screenHeight)
            }
            is Packet.VideoFrame -> {
                writeU32LE(out, packet.width)
                writeU32LE(out, packet.height)
                when (val f = packet.format) {
                    is FrameFormat.Jpeg -> {
                        writeU32LE(out, 0)
                        out.write(f.quality and 0xFF)
                    }
                    is FrameFormat.H264 -> {
                        writeU32LE(out, 1)
                        out.write(if (f.isKeyframe) 1 else 0)
                    }
                    is FrameFormat.RawRgba -> {
                        writeU32LE(out, 2)
                    }
                }
                writeBytes(out, packet.data)
                writeU64LE(out, packet.timestampMs)
            }
            is Packet.Input -> {
                writeInputEvent(out, packet.event)
            }
            is Packet.Clipboard -> {
                writeString(out, packet.text)
            }
            is Packet.Ping -> {
                writeU64LE(out, packet.timestampMs)
            }
            is Packet.Pong -> {
                writeU64LE(out, packet.timestampMs)
            }
            is Packet.Audio -> {
                writeU32LE(out, packet.sampleRate)
                out.write(packet.channels and 0xFF)
                out.write((packet.channels shr 8) and 0xFF)
                writeBytes(out, packet.data)
            }
            is Packet.SetQuality -> {
                out.write(packet.mode and 0xFF)
            }
        }
    }

    private fun writeInputEvent(out: ByteArrayOutputStream, event: InputEvent) {
        writeU32LE(out, event.variantIndex)
        when (event) {
            is InputEvent.MouseMove -> {
                writeF32LE(out, event.xRatio)
                writeF32LE(out, event.yRatio)
            }
            is InputEvent.MouseDown -> {
                writeU32LE(out, event.button.id)
            }
            is InputEvent.MouseUp -> {
                writeU32LE(out, event.button.id)
            }
            is InputEvent.MouseScroll -> {
                writeI32LE(out, event.deltaX)
                writeI32LE(out, event.deltaY)
            }
            is InputEvent.KeyDown -> {
                writeKeyCode(out, event.key)
            }
            is InputEvent.KeyUp -> {
                writeKeyCode(out, event.key)
            }
        }
    }

    private fun writeKeyCode(out: ByteArrayOutputStream, key: KeyCode) {
        writeU32LE(out, key.variantIndex)
        when (key) {
            is KeyCode.Unicode -> {
                // bincode serializes a Rust `char` as its UTF-8 encoding (1-4 bytes),
                // NOT as a 4-byte scalar value.
                out.write(String(Character.toChars(key.codePoint)).toByteArray(Charsets.UTF_8))
            }
            is KeyCode.F -> {
                out.write(key.num and 0xFF)
            }
            is KeyCode.Other -> {
                writeU32LE(out, key.code.toInt())
            }
            else -> {
                // Unit variants have no extra data
            }
        }
    }

    /**
     * Reads a complete Packet from InputStream using the 4-byte BE length prefix
     */
    fun readPacket(stream: InputStream): Packet {
        val lenBuf = ByteArray(4)
        readFully(stream, lenBuf)
        val len = ByteBuffer.wrap(lenBuf).order(ByteOrder.BIG_ENDIAN).int

        if (len < 0 || len > 50 * 1024 * 1024) {
            throw IllegalStateException("Invalid packet length: $len")
        }

        val payload = ByteArray(len)
        readFully(stream, payload)

        val inBuf = ByteBuffer.wrap(payload).order(ByteOrder.LITTLE_ENDIAN)
        val variantIndex = inBuf.int

        return when (variantIndex) {
            0 -> {
                val version = readString(inBuf)
                val token = readString(inBuf)
                Packet.AuthRequest(version, token)
            }
            1 -> {
                val success = inBuf.get() != 0.toByte()
                val message = readString(inBuf)
                val w = inBuf.int
                val h = inBuf.int
                Packet.AuthResponse(success, message, w, h)
            }
            2 -> {
                val w = inBuf.int
                val h = inBuf.int
                val fmtIndex = inBuf.int
                val format = when (fmtIndex) {
                    0 -> {
                        val quality = inBuf.get().toInt() and 0xFF
                        FrameFormat.Jpeg(quality)
                    }
                    1 -> {
                        val isKeyframe = inBuf.get() != 0.toByte()
                        FrameFormat.H264(isKeyframe)
                    }
                    else -> FrameFormat.RawRgba
                }
                val data = readBytes(inBuf)
                val timestamp = inBuf.long
                Packet.VideoFrame(w, h, format, data, timestamp)
            }
            3 -> {
                val event = readInputEvent(inBuf)
                Packet.Input(event)
            }
            4 -> {
                val text = readString(inBuf)
                Packet.Clipboard(text)
            }
            5 -> {
                val ts = inBuf.long
                Packet.Ping(ts)
            }
            6 -> {
                val ts = inBuf.long
                Packet.Pong(ts)
            }
            7 -> {
                val rate = inBuf.int
                val channels = inBuf.short.toInt() and 0xFFFF
                val data = readBytes(inBuf)
                Packet.Audio(rate, channels, data)
            }
            else -> throw IllegalStateException("Unknown packet variant index: $variantIndex")
        }
    }

    private fun readInputEvent(buf: ByteBuffer): InputEvent {
        val idx = buf.int
        return when (idx) {
            0 -> {
                val x = buf.float
                val y = buf.float
                InputEvent.MouseMove(x, y)
            }
            1 -> {
                val btnId = buf.int
                InputEvent.MouseDown(MouseBtn.entries.firstOrNull { it.id == btnId } ?: MouseBtn.LEFT)
            }
            2 -> {
                val btnId = buf.int
                InputEvent.MouseUp(MouseBtn.entries.firstOrNull { it.id == btnId } ?: MouseBtn.LEFT)
            }
            3 -> {
                val dx = buf.int
                val dy = buf.int
                InputEvent.MouseScroll(dx, dy)
            }
            4 -> {
                val key = readKeyCode(buf)
                InputEvent.KeyDown(key)
            }
            5 -> {
                val key = readKeyCode(buf)
                InputEvent.KeyUp(key)
            }
            else -> InputEvent.MouseMove(0f, 0f)
        }
    }

    private fun readKeyCode(buf: ByteBuffer): KeyCode {
        val idx = buf.int
        return when (idx) {
            0 -> KeyCode.Unicode(readUtf8CodePoint(buf))
            1 -> KeyCode.Backspace
            2 -> KeyCode.Tab
            3 -> KeyCode.Enter
            4 -> KeyCode.Escape
            5 -> KeyCode.Space
            6 -> KeyCode.Shift
            7 -> KeyCode.Control
            8 -> KeyCode.Alt
            9 -> KeyCode.Meta
            10 -> KeyCode.CapsLock
            11 -> KeyCode.LeftArrow
            12 -> KeyCode.RightArrow
            13 -> KeyCode.UpArrow
            14 -> KeyCode.DownArrow
            15 -> KeyCode.Delete
            16 -> KeyCode.Home
            17 -> KeyCode.End
            18 -> KeyCode.PageUp
            19 -> KeyCode.PageDown
            20 -> KeyCode.F(buf.get().toInt() and 0xFF)
            21 -> KeyCode.Other(buf.int.toLong())
            else -> KeyCode.Space
        }
    }

    private fun readUtf8CodePoint(buf: ByteBuffer): Int {
        val b0 = buf.get().toInt() and 0xFF
        val extra = when {
            b0 < 0x80 -> 0
            b0 < 0xE0 -> 1
            b0 < 0xF0 -> 2
            else -> 3
        }
        val bytes = ByteArray(1 + extra)
        bytes[0] = b0.toByte()
        for (i in 1..extra) bytes[i] = buf.get()
        return String(bytes, Charsets.UTF_8).codePointAt(0)
    }

    private fun readFully(stream: InputStream, b: ByteArray) {
        var offset = 0
        while (offset < b.size) {
            val count = stream.read(b, offset, b.size - offset)
            if (count < 0) throw IllegalStateException("Unexpected end of stream")
            offset += count
        }
    }

    private fun writeU32LE(out: ByteArrayOutputStream, value: Int) {
        out.write(value and 0xFF)
        out.write((value shr 8) and 0xFF)
        out.write((value shr 16) and 0xFF)
        out.write((value shr 24) and 0xFF)
    }

    private fun writeI32LE(out: ByteArrayOutputStream, value: Int) {
        writeU32LE(out, value)
    }

    private fun writeF32LE(out: ByteArrayOutputStream, value: Float) {
        writeU32LE(out, java.lang.Float.floatToIntBits(value))
    }

    private fun writeU64LE(out: ByteArrayOutputStream, value: Long) {
        for (i in 0..7) {
            out.write(((value shr (i * 8)) and 0xFF).toInt())
        }
    }

    private fun writeString(out: ByteArrayOutputStream, text: String) {
        val bytes = text.toByteArray(Charsets.UTF_8)
        writeU64LE(out, bytes.size.toLong())
        out.write(bytes)
    }

    private fun writeBytes(out: ByteArrayOutputStream, data: ByteArray) {
        writeU64LE(out, data.size.toLong())
        out.write(data)
    }

    private fun readString(buf: ByteBuffer): String {
        val len = buf.long.toInt()
        val bytes = ByteArray(len)
        buf.get(bytes)
        return String(bytes, Charsets.UTF_8)
    }

    private fun readBytes(buf: ByteBuffer): ByteArray {
        val len = buf.long.toInt()
        val bytes = ByteArray(len)
        buf.get(bytes)
        return bytes
    }
}

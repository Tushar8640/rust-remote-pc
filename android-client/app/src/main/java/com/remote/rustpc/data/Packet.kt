package com.remote.rustpc.data

const val PROTOCOL_VERSION = "0.1.0"
const val DEFAULT_PORT = 5885

/**
 * Frame formats matching Rust host
 */
sealed class FrameFormat {
    data class Jpeg(val quality: Int) : FrameFormat()
    data class H264(val isKeyframe: Boolean) : FrameFormat()
    data object RawRgba : FrameFormat()
}

/**
 * Mouse buttons matching Rust host
 */
enum class MouseBtn(val id: Int) {
    LEFT(0),
    RIGHT(1),
    MIDDLE(2)
}

/**
 * KeyCode matching Rust KeyCode enum
 */
sealed class KeyCode(val variantIndex: Int) {
    /** A Unicode scalar value (code point). Serialized as UTF-8, like Rust's `char` in bincode. */
    data class Unicode(val codePoint: Int) : KeyCode(0)
    data object Backspace : KeyCode(1)
    data object Tab : KeyCode(2)
    data object Enter : KeyCode(3)
    data object Escape : KeyCode(4)
    data object Space : KeyCode(5)
    data object Shift : KeyCode(6)
    data object Control : KeyCode(7)
    data object Alt : KeyCode(8)
    data object Meta : KeyCode(9) // Windows key
    data object CapsLock : KeyCode(10)
    data object LeftArrow : KeyCode(11)
    data object RightArrow : KeyCode(12)
    data object UpArrow : KeyCode(13)
    data object DownArrow : KeyCode(14)
    data object Delete : KeyCode(15)
    data object Home : KeyCode(16)
    data object End : KeyCode(17)
    data object PageUp : KeyCode(18)
    data object PageDown : KeyCode(19)
    data class F(val num: Int) : KeyCode(20)
    data class Other(val code: Long) : KeyCode(21)
}

/**
 * InputEvent matching Rust InputEvent enum
 */
sealed class InputEvent(val variantIndex: Int) {
    data class MouseMove(val xRatio: Float, val yRatio: Float) : InputEvent(0)
    data class MouseDown(val button: MouseBtn) : InputEvent(1)
    data class MouseUp(val button: MouseBtn) : InputEvent(2)
    data class MouseScroll(val deltaX: Int, val deltaY: Int) : InputEvent(3)
    data class KeyDown(val key: KeyCode) : InputEvent(4)
    data class KeyUp(val key: KeyCode) : InputEvent(5)
}

/**
 * Top-level network message between Host PC and Mobile Client
 */
sealed class Packet(val variantIndex: Int) {
    data class AuthRequest(
        val clientVersion: String,
        val tokenHash: String
    ) : Packet(0)

    data class AuthResponse(
        val success: Boolean,
        val message: String,
        val screenWidth: Int,
        val screenHeight: Int
    ) : Packet(1)

    data class VideoFrame(
        val width: Int,
        val height: Int,
        val format: FrameFormat,
        val data: ByteArray,
        val timestampMs: Long
    ) : Packet(2)

    data class Input(val event: InputEvent) : Packet(3)

    data class Clipboard(val text: String) : Packet(4)

    data class Ping(val timestampMs: Long) : Packet(5)

    data class Pong(val timestampMs: Long) : Packet(6)

    /** Host system audio: interleaved signed 16-bit little-endian PCM, 1 or 2 channels. */
    class Audio(
        val sampleRate: Int,
        val channels: Int,
        val data: ByteArray
    ) : Packet(7)

    /** Ask the host to change the video quality mode (see [QualityMode]) for this session. */
    data class SetQuality(val mode: Int) : Packet(8)
}

/** Video quality modes. The id is the byte sent on the wire (matches Rust `QualityMode`). */
enum class QualityMode(val id: Int, val label: String, val hint: String) {
    AUTO(0, "Auto", "Balanced, adapts to your connection"),
    SMOOTH(1, "Smooth", "Lowest delay and highest frame rate"),
    SHARP(2, "Sharp", "Best picture, may lower frame rate"),
    SAVER(3, "Data saver", "Lowest data use, for mobile data");

    companion object {
        fun fromId(id: Int): QualityMode = entries.firstOrNull { it.id == id } ?: AUTO
    }
}

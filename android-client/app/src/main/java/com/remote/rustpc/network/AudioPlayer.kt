package com.remote.rustpc.network

import android.media.AudioAttributes
import android.media.AudioFormat
import android.media.AudioTrack
import android.util.Log

/**
 * Plays the host PC's audio (16-bit little-endian PCM, mono or stereo).
 *
 * [write] blocks while the track's buffer is full, so call it from a dedicated
 * coroutine/thread and never from the network read loop.
 */
class AudioPlayer {

    private val tag = "AudioPlayer"
    private var track: AudioTrack? = null
    private var rate = 0
    private var channels = 0

    @Volatile
    private var gain = 1f

    @Volatile
    private var muted = false

    fun setVolume(volume: Float) {
        gain = volume.coerceIn(0f, 1f)
        applyVolume()
    }

    fun setMuted(value: Boolean) {
        muted = value
        applyVolume()
    }

    private fun applyVolume() {
        try {
            track?.setVolume(if (muted) 0f else gain)
        } catch (_: Exception) {
        }
    }

    fun write(sampleRate: Int, channelCount: Int, pcm: ByteArray) {
        if (pcm.isEmpty() || sampleRate <= 0 || channelCount !in 1..2) return
        if (track == null || rate != sampleRate || channels != channelCount) {
            recreate(sampleRate, channelCount)
        }
        val t = track ?: return
        try {
            t.write(pcm, 0, pcm.size)
        } catch (e: Exception) {
            Log.w(tag, "Audio write failed: ${e.message}")
        }
    }

    private fun recreate(sampleRate: Int, channelCount: Int) {
        release()
        val mask = if (channelCount == 1) AudioFormat.CHANNEL_OUT_MONO else AudioFormat.CHANNEL_OUT_STEREO
        val minBuf = AudioTrack.getMinBufferSize(sampleRate, mask, AudioFormat.ENCODING_PCM_16BIT)
        if (minBuf <= 0) {
            Log.w(tag, "Unsupported audio format: $sampleRate Hz, $channelCount ch")
            return
        }
        // ~120 ms of audio: enough to ride out network jitter without adding noticeable delay.
        val target = sampleRate * channelCount * 2 * 120 / 1000
        val bufSize = maxOf(minBuf, target)

        try {
            val t = AudioTrack.Builder()
                .setAudioAttributes(
                    AudioAttributes.Builder()
                        .setUsage(AudioAttributes.USAGE_MEDIA)
                        .setContentType(AudioAttributes.CONTENT_TYPE_MOVIE)
                        .build()
                )
                .setAudioFormat(
                    AudioFormat.Builder()
                        .setSampleRate(sampleRate)
                        .setChannelMask(mask)
                        .setEncoding(AudioFormat.ENCODING_PCM_16BIT)
                        .build()
                )
                .setBufferSizeInBytes(bufSize)
                .setTransferMode(AudioTrack.MODE_STREAM)
                .setPerformanceMode(AudioTrack.PERFORMANCE_MODE_LOW_LATENCY)
                .build()
            t.play()
            track = t
            rate = sampleRate
            channels = channelCount
            applyVolume()
        } catch (e: Exception) {
            Log.e(tag, "Could not create AudioTrack: ${e.message}")
            track = null
        }
    }

    fun release() {
        try {
            track?.stop()
        } catch (_: Exception) {
        }
        try {
            track?.release()
        } catch (_: Exception) {
        }
        track = null
        rate = 0
        channels = 0
    }
}

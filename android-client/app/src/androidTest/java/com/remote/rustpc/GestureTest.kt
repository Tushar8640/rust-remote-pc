package com.remote.rustpc

import android.graphics.Bitmap
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.test.center
import androidx.compose.ui.test.click
import androidx.compose.ui.test.down
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.moveBy
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.up
import androidx.compose.ui.unit.dp
import com.remote.rustpc.data.MouseBtn
import com.remote.rustpc.network.RemoteInput
import com.remote.rustpc.ui.components.RemoteCanvas
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/** Records what the canvas would send to the PC. */
private class RecordingInput : RemoteInput {
    val events = mutableListOf<String>()
    val clicks get() = events.count { it == "click LEFT" }
    val scrollY get() = events.filter { it.startsWith("scroll") }.sumOf { it.split(' ')[2].toInt() }
    val scrollX get() = events.filter { it.startsWith("scroll") }.sumOf { it.split(' ')[1].toInt() }
    val scrolls get() = events.count { it.startsWith("scroll") }

    override fun sendMouseMove(xRatio: Float, yRatio: Float) { events += "move %.4f %.4f".format(xRatio, yRatio) }
    override fun sendMouseDown(button: MouseBtn) { events += "down $button" }
    override fun sendMouseUp(button: MouseBtn) { events += "up $button" }
    override fun sendMouseClick(button: MouseBtn) { events += "click $button" }
    override fun sendMouseScroll(deltaX: Int, deltaY: Int) { events += "scroll $deltaX $deltaY" }
}

/**
 * Drives the real touch handling in [RemoteCanvas] with synthetic fingers and checks
 * what would reach the PC.
 */
class GestureTest {
    @get:Rule
    val rule = createComposeRule()

    private val input = RecordingInput()
    private val transforms = mutableListOf<Pair<Float, Offset>>()

    private fun show(direct: Boolean = true) {
        val bitmap = Bitmap.createBitmap(1920, 1080, Bitmap.Config.ARGB_8888)
        rule.setContent {
            // 16:9, so the picture fills the canvas exactly.
            Box(Modifier.size(640.dp, 360.dp)) {
                RemoteCanvas(
                    bitmap = bitmap,
                    connection = input,
                    isDirectTouch = direct,
                    scale = 1f,
                    offset = Offset.Zero,
                    onTransform = { zoom, pan -> transforms += zoom to pan }
                )
            }
        }
        rule.waitForIdle()
    }

    @Test
    fun singleTapIsOneClick() {
        show()
        rule.onRoot().performTouchInput { click(center) }
        assertEquals(1, input.clicks)
        assertEquals(0, input.scrolls)
        assertTrue(transforms.isEmpty())
    }

    @Test
    fun twoQuickTapsAreTwoClicksAtTheSamePlace() {
        show()
        rule.onRoot().performTouchInput {
            click(center)
            // The second tap lands a few millimetres off, as a finger really would.
            click(center + Offset(18f, 12f))
        }
        assertEquals(2, input.clicks)
        // The pointer is put back on the first click's spot before the second click, so
        // Windows sees two clicks in the same place and treats them as a double click.
        val clickIdx = input.events.indices.filter { input.events[it] == "click LEFT" }
        val moveBeforeFirst = input.events[clickIdx[0] - 1]
        val moveBeforeSecond = input.events[clickIdx[1] - 1]
        assertTrue(moveBeforeFirst.startsWith("move"))
        assertEquals(moveBeforeFirst, moveBeforeSecond)
    }

    @Test
    fun twoFingersDownScrollsUpWithoutClicking() {
        show()
        rule.onRoot().performTouchInput {
            down(0, Offset(width * 0.3f, height * 0.3f))
            down(1, Offset(width * 0.6f, height * 0.3f))
            repeat(12) { moveBy(0, Offset(0f, 16f)); moveBy(1, Offset(0f, 16f)) }
            up(0)
            up(1)
        }
        assertTrue("expected scroll events; events=${input.events.take(12)} transforms=${transforms.take(6)}", input.scrolls > 0)
        assertTrue("fingers moving down scroll the page up (positive), got ${input.scrollY}", input.scrollY >= 120)
        assertTrue("no sideways scroll, got ${input.scrollX}", kotlin.math.abs(input.scrollX) < 30)
        assertEquals(0, input.clicks)
        assertTrue("a scroll must not zoom", transforms.isEmpty())
    }

    @Test
    fun twoFingersUpScrollsDown() {
        show()
        rule.onRoot().performTouchInput {
            down(0, Offset(width * 0.3f, height * 0.7f))
            down(1, Offset(width * 0.6f, height * 0.7f))
            repeat(12) { moveBy(0, Offset(0f, -16f)); moveBy(1, Offset(0f, -16f)) }
            up(0)
            up(1)
        }
        assertTrue("got ${input.scrollY}", input.scrollY <= -120)
        assertEquals(0, input.clicks)
        assertTrue(transforms.isEmpty())
    }

    @Test
    fun twoFingersLeftScrollsRight() {
        show()
        rule.onRoot().performTouchInput {
            down(0, Offset(width * 0.6f, height * 0.5f))
            down(1, Offset(width * 0.8f, height * 0.5f))
            repeat(12) { moveBy(0, Offset(-16f, 0f)); moveBy(1, Offset(-16f, 0f)) }
            up(0)
            up(1)
        }
        assertTrue("got ${input.scrollX}", input.scrollX >= 120)
        assertTrue(kotlin.math.abs(input.scrollY) < 30)
        assertTrue(transforms.isEmpty())
    }

    @Test
    fun scrollAlsoWorksInTrackpadMode() {
        show(direct = false)
        rule.onRoot().performTouchInput {
            down(0, Offset(width * 0.3f, height * 0.3f))
            down(1, Offset(width * 0.6f, height * 0.3f))
            repeat(12) { moveBy(0, Offset(0f, 16f)); moveBy(1, Offset(0f, 16f)) }
            up(0)
            up(1)
        }
        assertTrue(input.scrollY >= 120)
        assertEquals(0, input.clicks)
    }

    @Test
    fun pinchZoomsAndDoesNotScroll() {
        show()
        rule.onRoot().performTouchInput {
            down(0, Offset(width * 0.45f, height * 0.5f))
            down(1, Offset(width * 0.55f, height * 0.5f))
            repeat(12) { moveBy(0, Offset(-14f, 0f)); moveBy(1, Offset(14f, 0f)) }
            up(0)
            up(1)
        }
        assertTrue("expected zoom updates", transforms.isNotEmpty())
        assertTrue("zoom should grow, got ${transforms.map { it.first }}", transforms.any { it.first > 1f })
        assertEquals(0, input.scrolls)
        assertEquals(0, input.clicks)
    }

    @Test
    fun tinyTwoFingerJitterDoesNothing() {
        show()
        rule.onRoot().performTouchInput {
            down(0, Offset(width * 0.4f, height * 0.5f))
            down(1, Offset(width * 0.6f, height * 0.5f))
            moveBy(0, Offset(2f, 1f))
            moveBy(1, Offset(2f, 1f))
            up(0)
            up(1)
        }
        assertEquals(0, input.scrolls)
        assertEquals(0, input.clicks)
    }

    @Test
    fun oneFingerDragStillDragsWithTheLeftButton() {
        show()
        rule.onRoot().performTouchInput {
            down(0, center)
            repeat(6) { moveBy(0, Offset(20f, 0f)) }
            up(0)
        }
        assertTrue(input.events.contains("down LEFT"))
        assertTrue(input.events.contains("up LEFT"))
        assertEquals(0, input.scrolls)
        assertEquals(0, input.clicks)
    }
}

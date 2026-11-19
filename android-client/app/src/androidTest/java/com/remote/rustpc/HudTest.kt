package com.remote.rustpc

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performSemanticsAction
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.swipe
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import com.remote.rustpc.network.ConnectionStats
import com.remote.rustpc.ui.components.HUD_MIN_OPACITY
import com.remote.rustpc.ui.components.HudOverlay
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/** The draggable ping/fps badge: gear opens the opacity panel, drag moves it, it stays on screen. */
class HudTest {
    @get:Rule
    val rule = createComposeRule()

    private var opacity = 0.9f
    private var offset = Offset.Zero
    private var lastFinal: Pair<Float, Offset>? = null
    private var boxSizePx = IntSize.Zero
    private val changes = mutableListOf<Float>()

    private fun handleBounds() = rule.onNodeWithTag("hud_handle").fetchSemanticsNode().boundsInRoot

    private fun show() {
        rule.setContent {
            var box by remember { mutableStateOf(IntSize.Zero) }
            var alpha by remember { mutableStateOf(opacity) }
            var pos by remember { mutableStateOf(offset) }
            Box(
                Modifier
                    .size(400.dp, 700.dp)
                    .onSizeChanged { box = it; boxSizePx = it }
            ) {
                HudOverlay(
                    stats = ConnectionStats(fps = 30, rttMs = 12, screenWidth = 1920, screenHeight = 1080),
                    audioActive = false,
                    audioMuted = false,
                    opacity = alpha,
                    offset = pos,
                    containerSize = box,
                    onChange = { a, p, final ->
                        changes += a
                        alpha = a
                        pos = p
                        opacity = a
                        offset = p
                        if (final) lastFinal = a to p
                    },
                    modifier = Modifier.align(Alignment.TopCenter)
                )
            }
        }
        rule.waitForIdle()
    }

    @Test
    fun gearOpensAndClosesTheOpacityPanel() {
        show()
        rule.onNodeWithContentDescription("HUD opacity").assertDoesNotExist()
        rule.onNodeWithContentDescription("HUD settings").performClick()
        rule.onNodeWithContentDescription("HUD opacity").assertExists()
        rule.onNodeWithText("Done").performClick()
        rule.onNodeWithContentDescription("HUD opacity").assertDoesNotExist()
    }

    @Test
    fun sliderChangesOpacityAndItIsClamped() {
        show()
        rule.onNodeWithContentDescription("HUD settings").performClick()
        var handled = false
        rule.onNodeWithContentDescription("HUD opacity")
            .performSemanticsAction(SemanticsActions.SetProgress) { handled = it(0.4f) }
        rule.waitForIdle()
        assertEquals("handled=$handled changes=$changes", 0.4f, opacity, 0.02f)

        // Asking for less than the minimum never makes the badge vanish.
        rule.onNodeWithContentDescription("HUD opacity")
            .performSemanticsAction(SemanticsActions.SetProgress) { it(0.0f) }
        rule.waitForIdle()
        assertTrue("opacity ${opacity} should not go below $HUD_MIN_OPACITY", opacity >= HUD_MIN_OPACITY - 0.001f)
    }

    @Test
    fun draggingMovesTheBadgeAndReportsTheFinalSpot() {
        show()
        val before = handleBounds().top
        rule.onNodeWithTag("hud_handle").performTouchInput {
            swipe(center, center + Offset(0f, 300f), durationMillis = 200)
        }
        rule.waitForIdle()
        val after = handleBounds().top
        assertTrue("moved down, was $before now $after", after > before + 150f)
        val saved = lastFinal
        assertTrue("final position is reported when the drag ends", saved != null && saved.second.y > 150f)
    }

    @Test
    fun theBadgeCannotBeDraggedOffScreen() {
        show()
        rule.onNodeWithTag("hud_handle").performTouchInput {
            swipe(center, center + Offset(5000f, 5000f), durationMillis = 200)
        }
        rule.waitForIdle()
        val b = handleBounds()
        assertTrue("badge $b stays inside ${boxSizePx.width}x${boxSizePx.height}",
            b.left >= -1f && b.top >= -1f && b.right <= boxSizePx.width + 1f && b.bottom <= boxSizePx.height + 1f)
        // ... and it really did travel to the far corner.
        assertTrue("badge $b should reach the bottom right", b.right > boxSizePx.width - 40f && b.bottom > boxSizePx.height - 40f)
    }

    @Test
    fun resetPositionPutsItBack() {
        show()
        rule.onNodeWithTag("hud_handle").performTouchInput {
            swipe(center, center + Offset(0f, 300f), durationMillis = 200)
        }
        rule.waitForIdle()
        rule.onNodeWithContentDescription("HUD settings").performClick()
        rule.onNodeWithText("Reset position").performClick()
        rule.waitForIdle()
        assertEquals(Offset.Zero, offset)
        assertEquals(Offset.Zero, lastFinal?.second)
    }
}

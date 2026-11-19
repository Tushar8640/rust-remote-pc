package com.remote.rustpc.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material3.Icon
import androidx.compose.material3.Slider
import androidx.compose.material3.SliderDefaults
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.layout.positionInRoot
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.remote.rustpc.network.ConnectionStats
import com.remote.rustpc.ui.theme.*
import kotlin.math.max
import kotlin.math.roundToInt

const val HUD_MIN_OPACITY = 0.15f

/**
 * The ping/fps badge with a settings button beside it.
 *
 * * Drag the badge anywhere on the screen; it stays inside the screen.
 * * The gear opens a small panel with an opacity slider (the badge can be made nearly
 *   invisible, and the gear stays easy to find) and a Reset position button.
 *
 * State is hoisted: [opacity] and [offset] come from the caller (which saves them) and
 * [onChange] reports changes. `final` is false while a drag/slider is in progress and true
 * when it ends, so callers can save only once.
 *
 * Place it with `Modifier.align(...)` and pass the size of the area it may move in as
 * [containerSize]. [offset] is relative to wherever the caller placed it.
 */
@Composable
fun HudOverlay(
    stats: ConnectionStats,
    audioActive: Boolean,
    audioMuted: Boolean,
    opacity: Float,
    offset: Offset,
    containerSize: IntSize,
    onChange: (opacity: Float, offset: Offset, final: Boolean) -> Unit,
    modifier: Modifier = Modifier
) {
    var settingsOpen by remember { mutableStateOf(false) }
    var size by remember { mutableStateOf(IntSize.Zero) }
    // Where this would sit with no offset, in window coordinates (to keep it on screen).
    var base by remember { mutableStateOf(Offset.Zero) }
    // Not keyed on [offset]: re-keying while dragging would orphan the drag handler's state.
    var current by remember { mutableStateOf(offset) }
    LaunchedEffect(offset) { if (offset != current) current = offset }

    val latestContainer by rememberUpdatedState(containerSize)
    val latestSize by rememberUpdatedState(size)
    val latestBase by rememberUpdatedState(base)
    // The newest opacity, updated at once (not on the next recomposition), so a value change
    // followed immediately by "finished" saves the new value, not the previous one.
    var liveOpacity by remember { mutableFloatStateOf(opacity) }
    LaunchedEffect(opacity) { liveOpacity = opacity }

    fun clamp(o: Offset): Offset {
        val c = latestContainer
        val s = latestSize
        if (c.width == 0 || s.width == 0) return o
        val b = latestBase
        return Offset(
            o.x.coerceIn(-b.x, (c.width - s.width - b.x).coerceAtLeast(-b.x)),
            o.y.coerceIn(-b.y, (c.height - s.height - b.y).coerceAtLeast(-b.y))
        )
    }

    Column(
        modifier = modifier
            .offset { IntOffset(current.x.roundToInt(), current.y.roundToInt()) }
            .onSizeChanged { size = it }
            .onGloballyPositioned { base = it.positionInRoot() - current },
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(8.dp)
    ) {
        Row(
            modifier = Modifier
                .testTag("hud_handle")
                .pointerInput(Unit) {
                    detectDragGestures(
                        onDragEnd = { onChange(liveOpacity, current, true) },
                        onDragCancel = { onChange(liveOpacity, current, true) }
                    ) { change, drag ->
                        change.consume()
                        current = clamp(current + drag)
                        onChange(liveOpacity, current, false)
                    }
                },
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(6.dp)
        ) {
            StatBadge(
                stats = stats,
                audioActive = audioActive,
                audioMuted = audioMuted,
                alpha = opacity
            )
            // The gear never fades out completely, so it can always be found again.
            Box(
                modifier = Modifier
                    .alpha(max(opacity, 0.6f))
                    .size(32.dp)
                    .clip(CircleShape)
                    .background(BgDark.copy(alpha = 0.82f))
                    .border(1.dp, BorderDark, CircleShape)
                    .semantics { contentDescription = "HUD settings" }
                    .clickable { settingsOpen = !settingsOpen },
                contentAlignment = Alignment.Center
            ) {
                Icon(
                    Icons.Default.Settings,
                    contentDescription = null,
                    tint = if (settingsOpen) AccentIndigoLight else TextSecondary,
                    modifier = Modifier.size(18.dp)
                )
            }
        }

        if (settingsOpen) {
            Surface(
                shape = RoundedCornerShape(16.dp),
                color = SurfaceDark.copy(alpha = 0.96f),
                border = androidx.compose.foundation.BorderStroke(1.dp, BorderDark)
            ) {
                Column(
                    modifier = Modifier
                        .width(240.dp)
                        .padding(horizontal = 14.dp, vertical = 10.dp)
                ) {
                    Text(
                        "Opacity  ${(opacity * 100).roundToInt()}%",
                        style = Typography.labelMedium.copy(color = TextPrimary, fontSize = 12.sp)
                    )
                    Slider(
                        value = opacity,
                        onValueChange = {
                            liveOpacity = it.coerceIn(HUD_MIN_OPACITY, 1f)
                            onChange(liveOpacity, current, false)
                        },
                        onValueChangeFinished = { onChange(liveOpacity, current, true) },
                        valueRange = HUD_MIN_OPACITY..1f,
                        modifier = Modifier
                            .fillMaxWidth()
                            .semantics { contentDescription = "HUD opacity" },
                        colors = SliderDefaults.colors(
                            thumbColor = AccentIndigoLight,
                            activeTrackColor = AccentIndigo,
                            inactiveTrackColor = SurfaceElevated
                        )
                    )
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween
                    ) {
                        TextButton(onClick = {
                            current = Offset.Zero
                            onChange(liveOpacity, Offset.Zero, true)
                        }) {
                            Text("Reset position", color = TextSecondary, fontSize = 12.sp)
                        }
                        TextButton(onClick = { settingsOpen = false }) {
                            Text("Done", color = AccentIndigoLight, fontSize = 12.sp)
                        }
                    }
                }
            }
        }
    }
}

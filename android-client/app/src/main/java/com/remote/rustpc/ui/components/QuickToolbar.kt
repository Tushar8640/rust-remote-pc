package com.remote.rustpc.ui.components

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.VolumeOff
import androidx.compose.material.icons.automirrored.filled.VolumeUp
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.remote.rustpc.data.KeyCode
import com.remote.rustpc.data.MouseBtn
import com.remote.rustpc.data.QualityMode
import com.remote.rustpc.network.RemoteConnection
import com.remote.rustpc.ui.theme.*

@Composable
fun QuickToolbar(
    connection: RemoteConnection,
    isDirectTouch: Boolean,
    onToggleTouchMode: () -> Unit,
    onResetZoom: () -> Unit,
    onOpenKeyboard: () -> Unit,
    audioMuted: Boolean,
    audioVolume: Float,
    audioActive: Boolean,
    isFullscreen: Boolean,
    onToggleFullscreen: () -> Unit,
    isLandscapeLocked: Boolean,
    onToggleRotation: () -> Unit,
    onHide: () -> Unit,
    onDisconnect: () -> Unit,
    modifier: Modifier = Modifier
) {
    var isExpanded by remember { mutableStateOf(false) }

    Column(
        modifier = modifier.widthIn(max = 560.dp),
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        // Expanded panel: keys, mouse buttons, audio
        AnimatedVisibility(
            visible = isExpanded,
            enter = fadeIn() + expandVertically(),
            exit = fadeOut() + shrinkVertically()
        ) {
            Surface(
                shape = RoundedCornerShape(18.dp),
                color = SurfaceDark.copy(alpha = 0.96f),
                border = BorderStroke(1.dp, BorderDark),
                modifier = Modifier.padding(bottom = 10.dp)
            ) {
                Column(
                    modifier = Modifier.padding(12.dp),
                    verticalArrangement = Arrangement.spacedBy(10.dp)
                ) {
                    PanelLabel("Keys")
                    Row(
                        modifier = Modifier.horizontalScroll(rememberScrollState()),
                        horizontalArrangement = Arrangement.spacedBy(6.dp)
                    ) {
                        KeyChip("Win") { connection.sendKeyClick(KeyCode.Meta) }
                        KeyChip("Esc") { connection.sendKeyClick(KeyCode.Escape) }
                        KeyChip("Tab") { connection.sendKeyClick(KeyCode.Tab) }
                        KeyChip("Enter") { connection.sendKeyClick(KeyCode.Enter) }
                        KeyChip("Bksp") { connection.sendKeyClick(KeyCode.Backspace) }
                        KeyChip("Del") { connection.sendKeyClick(KeyCode.Delete) }
                        KeyChip("Ctrl+C") { connection.sendCtrlShortcut('c') }
                        KeyChip("Ctrl+V") { connection.sendCtrlShortcut('v') }
                        KeyChip("Ctrl+A") { connection.sendCtrlShortcut('a') }
                        KeyChip("Ctrl+Z") { connection.sendCtrlShortcut('z') }
                    }

                    PanelLabel("Mouse")
                    Row(
                        modifier = Modifier.horizontalScroll(rememberScrollState()),
                        horizontalArrangement = Arrangement.spacedBy(6.dp)
                    ) {
                        KeyChip("Left click") { connection.sendMouseClick(MouseBtn.LEFT) }
                        KeyChip("Right click") { connection.sendMouseClick(MouseBtn.RIGHT) }
                        KeyChip("Double click") { connection.sendMouseDoubleClick(MouseBtn.LEFT) }
                        KeyChip("Scroll up") { connection.sendMouseScroll(0, 120) }
                        KeyChip("Scroll down") { connection.sendMouseScroll(0, -120) }
                        KeyChip("Reset zoom") { onResetZoom() }
                    }

                    PanelLabel("Quality")
                    val quality by connection.qualityMode.collectAsState()
                    Row(
                        modifier = Modifier.horizontalScroll(rememberScrollState()),
                        horizontalArrangement = Arrangement.spacedBy(6.dp)
                    ) {
                        QualityMode.entries.forEach { m ->
                            KeyChip(
                                text = if (m == quality) "✓ ${m.label}" else m.label,
                                onClick = { connection.setQuality(m) }
                            )
                        }
                    }
                    Text(quality.hint, style = Typography.labelMedium.copy(color = TextFaint))

                    PanelLabel("Audio")
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(8.dp)
                    ) {
                        Icon(
                            imageVector = if (audioMuted || audioVolume == 0f) Icons.AutoMirrored.Filled.VolumeOff else Icons.AutoMirrored.Filled.VolumeUp,
                            contentDescription = null,
                            tint = if (audioMuted) TextFaint else TextPrimary,
                            modifier = Modifier.size(20.dp)
                        )
                        Slider(
                            value = if (audioMuted) 0f else audioVolume,
                            onValueChange = { connection.setVolume(it) },
                            modifier = Modifier.weight(1f),
                            colors = SliderDefaults.colors(
                                thumbColor = AccentIndigoLight,
                                activeTrackColor = AccentIndigo,
                                inactiveTrackColor = SurfaceElevated
                            )
                        )
                    }
                    if (!audioActive) {
                        Text(
                            "No audio received yet. Make sure Share audio is on at the PC and something is playing.",
                            style = Typography.labelMedium.copy(color = TextFaint)
                        )
                    }
                }
            }
        }

        // Main pill
        Surface(
            shape = RoundedCornerShape(28.dp),
            color = SurfaceDark.copy(alpha = 0.96f),
            border = BorderStroke(1.dp, BorderDark)
        ) {
            Row(
                modifier = Modifier
                    .horizontalScroll(rememberScrollState())
                    .padding(horizontal = 6.dp, vertical = 6.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(2.dp)
            ) {
                ToolbarButton(
                    icon = if (isExpanded) Icons.Default.ExpandMore else Icons.Default.ExpandLess,
                    label = "More",
                    active = isExpanded,
                    onClick = { isExpanded = !isExpanded }
                )
                ToolbarButton(Icons.Default.Keyboard, "Type", onClick = onOpenKeyboard)
                ToolbarButton(
                    icon = if (isDirectTouch) Icons.Default.TouchApp else Icons.Default.Mouse,
                    label = if (isDirectTouch) "Touch" else "Trackpad",
                    active = true,
                    onClick = onToggleTouchMode
                )
                ToolbarButton(
                    icon = if (audioMuted) Icons.AutoMirrored.Filled.VolumeOff else Icons.AutoMirrored.Filled.VolumeUp,
                    label = if (audioMuted) "Unmute" else "Sound",
                    active = !audioMuted && audioActive,
                    onClick = { connection.setMuted(!audioMuted) }
                )
                ToolbarButton(
                    icon = if (isFullscreen) Icons.Default.FullscreenExit else Icons.Default.Fullscreen,
                    label = if (isFullscreen) "Exit" else "Full screen",
                    active = isFullscreen,
                    onClick = onToggleFullscreen
                )
                ToolbarButton(
                    icon = if (isLandscapeLocked) Icons.Default.ScreenLockRotation else Icons.Default.ScreenRotation,
                    label = if (isLandscapeLocked) "Landscape" else "Rotate",
                    active = isLandscapeLocked,
                    onClick = onToggleRotation
                )
                ToolbarButton(Icons.Default.KeyboardArrowDown, "Hide", onClick = onHide)

                Spacer(Modifier.width(4.dp))
                IconButton(
                    onClick = onDisconnect,
                    modifier = Modifier
                        .size(40.dp)
                        .clip(CircleShape)
                        .background(AccentRed.copy(alpha = 0.16f))
                ) {
                    Icon(
                        imageVector = Icons.Default.Close,
                        contentDescription = "Disconnect",
                        tint = AccentRed
                    )
                }
            }
        }
    }
}

@Composable
private fun PanelLabel(text: String) {
    Text(
        text = text.uppercase(),
        style = Typography.labelMedium.copy(color = TextFaint, fontSize = 10.sp, letterSpacing = 1.sp)
    )
}

@Composable
private fun ToolbarButton(
    icon: ImageVector,
    label: String,
    active: Boolean = false,
    onClick: () -> Unit
) {
    Column(
        modifier = Modifier
            .clip(RoundedCornerShape(14.dp))
            .background(if (active) AccentIndigo.copy(alpha = 0.18f) else Color.Transparent)
            .clickable { onClick() }
            .padding(horizontal = 10.dp, vertical = 5.dp),
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        Icon(
            imageVector = icon,
            contentDescription = label,
            tint = if (active) AccentIndigoLight else TextPrimary,
            modifier = Modifier.size(22.dp)
        )
        Text(
            text = label,
            maxLines = 1,
            style = Typography.labelMedium.copy(
                fontSize = 10.sp,
                color = if (active) AccentIndigoLight else TextSecondary
            )
        )
    }
}

@Composable
private fun KeyChip(text: String, onClick: () -> Unit) {
    Box(
        modifier = Modifier
            .clip(RoundedCornerShape(10.dp))
            .background(SurfaceElevated)
            .clickable { onClick() }
            .padding(horizontal = 12.dp, vertical = 8.dp),
        contentAlignment = Alignment.Center
    ) {
        Text(
            text = text,
            maxLines = 1,
            style = Typography.labelMedium.copy(fontSize = 12.sp, color = TextPrimary)
        )
    }
}

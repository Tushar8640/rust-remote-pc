package com.remote.rustpc.ui.components

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
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
import com.remote.rustpc.network.RemoteConnection
import com.remote.rustpc.ui.theme.*

@Composable
fun QuickToolbar(
    connection: RemoteConnection,
    isDirectTouch: Boolean,
    onToggleTouchMode: () -> Unit,
    onResetZoom: () -> Unit,
    onOpenKeyboard: () -> Unit,
    onDisconnect: () -> Unit,
    modifier: Modifier = Modifier
) {
    var isExpanded by remember { mutableStateOf(false) }

    Column(
        modifier = modifier,
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        // Expanded Extra Function Keys row
        AnimatedVisibility(
            visible = isExpanded,
            enter = fadeIn(),
            exit = fadeOut()
        ) {
            Row(
                modifier = Modifier
                    .padding(bottom = 8.dp)
                    .clip(RoundedCornerShape(16.dp))
                    .background(SurfaceDark.copy(alpha = 0.92f))
                    .padding(horizontal = 8.dp, vertical = 6.dp)
                    .horizontalScroll(rememberScrollState()),
                horizontalArrangement = Arrangement.spacedBy(6.dp),
                verticalAlignment = Alignment.CenterVertically
            ) {
                KeyButton("Win") { connection.sendKeyClick(KeyCode.Meta) }
                KeyButton("Esc") { connection.sendKeyClick(KeyCode.Escape) }
                KeyButton("Tab") { connection.sendKeyClick(KeyCode.Tab) }
                KeyButton("Enter") { connection.sendKeyClick(KeyCode.Enter) }
                KeyButton("Bksp") { connection.sendKeyClick(KeyCode.Backspace) }
                KeyButton("Copy") {
                    connection.sendKeyDown(KeyCode.Control)
                    connection.sendKeyClick(KeyCode.Unicode('c'))
                    connection.sendKeyUp(KeyCode.Control)
                }
                KeyButton("Paste") {
                    connection.sendKeyDown(KeyCode.Control)
                    connection.sendKeyClick(KeyCode.Unicode('v'))
                    connection.sendKeyUp(KeyCode.Control)
                }
                KeyButton("Reset Zoom") { onResetZoom() }
            }
        }

        // Main pill bar
        Row(
            modifier = Modifier
                .clip(RoundedCornerShape(28.dp))
                .background(SurfaceDark.copy(alpha = 0.92f))
                .padding(horizontal = 8.dp, vertical = 6.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp)
        ) {
            // Toggle Extra Keys
            IconButton(
                onClick = { isExpanded = !isExpanded },
                modifier = Modifier.size(40.dp)
            ) {
                Icon(
                    imageVector = if (isExpanded) Icons.Default.ExpandMore else Icons.Default.ExpandLess,
                    contentDescription = "Keys",
                    tint = AccentCyan
                )
            }

            // Keyboard
            ToolbarButton(
                icon = Icons.Default.Keyboard,
                label = "Type",
                onClick = onOpenKeyboard
            )

            // Left Click
            ToolbarButton(
                icon = Icons.Default.TouchApp,
                label = "L-Click",
                onClick = { connection.sendMouseClick(MouseBtn.LEFT) }
            )

            // Right Click
            ToolbarButton(
                icon = Icons.Default.AdsClick,
                label = "R-Click",
                onClick = { connection.sendMouseClick(MouseBtn.RIGHT) }
            )

            // Touch mode toggle (Trackpad vs Direct)
            ToolbarButton(
                icon = if (isDirectTouch) Icons.Default.Fingerprint else Icons.Default.Mouse,
                label = if (isDirectTouch) "Direct" else "Trackpad",
                accent = isDirectTouch,
                onClick = onToggleTouchMode
            )

            // Disconnect
            IconButton(
                onClick = onDisconnect,
                modifier = Modifier
                    .size(40.dp)
                    .clip(CircleShape)
                    .background(AccentRed.copy(alpha = 0.15f))
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

@Composable
private fun ToolbarButton(
    icon: ImageVector,
    label: String,
    accent: Boolean = false,
    onClick: () -> Unit
) {
    Column(
        modifier = Modifier
            .clip(RoundedCornerShape(12.dp))
            .clickable { onClick() }
            .padding(horizontal = 10.dp, vertical = 4.dp),
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        Icon(
            imageVector = icon,
            contentDescription = label,
            tint = if (accent) AccentCyan else TextPrimary,
            modifier = Modifier.size(20.dp)
        )
        Text(
            text = label,
            style = Typography.labelSmall.copy(
                fontSize = 10.sp,
                color = if (accent) AccentCyan else TextSecondary
            )
        )
    }
}

@Composable
private fun KeyButton(
    text: String,
    onClick: () -> Unit
) {
    Box(
        modifier = Modifier
            .clip(RoundedCornerShape(8.dp))
            .background(SurfaceElevated)
            .clickable { onClick() }
            .padding(horizontal = 10.dp, vertical = 6.dp),
        contentAlignment = Alignment.Center
    ) {
        Text(
            text = text,
            style = Typography.labelSmall.copy(fontSize = 11.sp, color = TextPrimary)
        )
    }
}

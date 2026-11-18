package com.remote.rustpc.ui.screens

import android.graphics.Bitmap
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.Send
import androidx.compose.material.icons.filled.KeyboardArrowUp
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import com.remote.rustpc.data.KeyCode
import com.remote.rustpc.network.ConnectionStats
import com.remote.rustpc.network.RemoteConnection
import com.remote.rustpc.ui.components.QuickToolbar
import com.remote.rustpc.ui.components.RemoteCanvas
import com.remote.rustpc.ui.components.StatBadge
import com.remote.rustpc.ui.theme.*

@Composable
fun ViewerScreen(
    bitmap: Bitmap?,
    stats: ConnectionStats,
    connection: RemoteConnection,
    audioMuted: Boolean,
    audioVolume: Float,
    audioActive: Boolean,
    isFullscreen: Boolean,
    onToggleFullscreen: () -> Unit,
    isLandscapeLocked: Boolean,
    onToggleRotation: () -> Unit,
    onDisconnect: () -> Unit,
    modifier: Modifier = Modifier
) {
    var isDirectTouch by remember { mutableStateOf(true) }
    var scale by remember { mutableFloatStateOf(1f) }
    var offset by remember { mutableStateOf(Offset.Zero) }
    var toolbarVisible by remember { mutableStateOf(true) }

    var showKeyboardDialog by remember { mutableStateOf(false) }
    var inputText by remember { mutableStateOf("") }
    val clipboard = LocalClipboardManager.current

    Box(
        modifier = modifier
            .fillMaxSize()
            .background(androidx.compose.ui.graphics.Color.Black)
    ) {
        // Full-screen remote canvas
        RemoteCanvas(
            bitmap = bitmap,
            connection = connection,
            isDirectTouch = isDirectTouch,
            scale = scale,
            offset = offset,
            onTransform = { zoomChange, panChange ->
                scale = (scale * zoomChange).coerceIn(1f, 5f)
                offset = if (scale <= 1.01f) Offset.Zero else offset + panChange
            },
            modifier = Modifier.fillMaxSize()
        )

        // Waiting for first frame
        if (bitmap == null) {
            Box(
                modifier = Modifier.fillMaxSize(),
                contentAlignment = Alignment.Center
            ) {
                Column(
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.spacedBy(16.dp)
                ) {
                    CircularProgressIndicator(color = AccentIndigo)
                    Text(
                        text = "Receiving video…",
                        style = Typography.bodyMedium.copy(color = TextSecondary)
                    )
                }
            }
        }

        // Top status HUD
        StatBadge(
            stats = stats,
            audioActive = audioActive,
            audioMuted = audioMuted,
            modifier = Modifier
                .align(Alignment.TopCenter)
                .statusBarsPadding()
                .padding(top = 10.dp)
        )

        // Bottom floating toolbar (collapsible so it never blocks the screen)
        AnimatedVisibility(
            visible = toolbarVisible,
            enter = fadeIn() + slideInVertically { it / 2 },
            exit = fadeOut() + slideOutVertically { it / 2 },
            modifier = Modifier.align(Alignment.BottomCenter)
        ) {
            QuickToolbar(
                connection = connection,
                isDirectTouch = isDirectTouch,
                onToggleTouchMode = { isDirectTouch = !isDirectTouch },
                onResetZoom = {
                    scale = 1f
                    offset = Offset.Zero
                },
                onOpenKeyboard = { showKeyboardDialog = true },
                audioMuted = audioMuted,
                audioVolume = audioVolume,
                audioActive = audioActive,
                isFullscreen = isFullscreen,
                onToggleFullscreen = onToggleFullscreen,
                isLandscapeLocked = isLandscapeLocked,
                onToggleRotation = onToggleRotation,
                onHide = { toolbarVisible = false },
                onDisconnect = onDisconnect,
                modifier = Modifier
                    .navigationBarsPadding()
                    .padding(horizontal = 12.dp, vertical = 14.dp)
            )
        }

        // Small handle to bring the toolbar back
        AnimatedVisibility(
            visible = !toolbarVisible,
            enter = fadeIn(),
            exit = fadeOut(),
            modifier = Modifier.align(Alignment.BottomCenter)
        ) {
            Surface(
                shape = RoundedCornerShape(20.dp),
                color = SurfaceDark.copy(alpha = 0.9f),
                border = BorderStroke(1.dp, BorderDark),
                modifier = Modifier
                    .navigationBarsPadding()
                    .padding(bottom = 10.dp)
                    .clickable { toolbarVisible = true }
            ) {
                Row(
                    modifier = Modifier.padding(horizontal = 16.dp, vertical = 6.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(6.dp)
                ) {
                    Icon(
                        Icons.Default.KeyboardArrowUp,
                        contentDescription = "Show toolbar",
                        tint = TextSecondary,
                        modifier = Modifier.size(18.dp)
                    )
                    Text("Controls", style = Typography.labelMedium.copy(color = TextSecondary))
                }
            }
        }

        // Text input dialog
        if (showKeyboardDialog) {
            Dialog(onDismissRequest = { showKeyboardDialog = false }) {
                Surface(
                    modifier = Modifier.fillMaxWidth(),
                    shape = RoundedCornerShape(20.dp),
                    color = SurfaceDark,
                    border = BorderStroke(1.dp, BorderDark)
                ) {
                    Column(
                        modifier = Modifier.padding(20.dp),
                        verticalArrangement = Arrangement.spacedBy(14.dp)
                    ) {
                        Text(
                            text = "Type on the PC",
                            style = Typography.titleLarge.copy(color = TextPrimary)
                        )

                        OutlinedTextField(
                            value = inputText,
                            onValueChange = { inputText = it },
                            placeholder = { Text("Text to send…", color = TextFaint) },
                            shape = RoundedCornerShape(12.dp),
                            colors = OutlinedTextFieldDefaults.colors(
                                focusedBorderColor = AccentIndigo,
                                unfocusedBorderColor = BorderDark,
                                focusedTextColor = TextPrimary,
                                unfocusedTextColor = TextPrimary,
                                cursorColor = AccentIndigoLight
                            ),
                            modifier = Modifier.fillMaxWidth()
                        )

                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.spacedBy(8.dp)
                        ) {
                            DialogKey("Enter", Modifier.weight(1f)) { connection.sendKeyClick(KeyCode.Enter) }
                            DialogKey("Backspace", Modifier.weight(1f)) { connection.sendKeyClick(KeyCode.Backspace) }
                            DialogKey("Paste", Modifier.weight(1f)) {
                                // Send this phone's clipboard to the PC, then press Ctrl+V there.
                                val text = clipboard.getText()?.text
                                if (!text.isNullOrEmpty()) {
                                    connection.sendClipboard(text)
                                    connection.sendCtrlShortcut('v')
                                }
                            }
                        }

                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.spacedBy(8.dp)
                        ) {
                            TextButton(
                                onClick = { showKeyboardDialog = false },
                                modifier = Modifier.weight(1f)
                            ) {
                                Text("Close", color = TextSecondary)
                            }

                            Button(
                                onClick = {
                                    if (inputText.isNotEmpty()) {
                                        connection.sendText(inputText)
                                        inputText = ""
                                    }
                                    showKeyboardDialog = false
                                },
                                shape = RoundedCornerShape(12.dp),
                                modifier = Modifier.weight(1f),
                                colors = ButtonDefaults.buttonColors(
                                    containerColor = AccentIndigo,
                                    contentColor = TextPrimary
                                )
                            ) {
                                Icon(Icons.AutoMirrored.Filled.Send, contentDescription = null, modifier = Modifier.size(18.dp))
                                Spacer(modifier = Modifier.width(6.dp))
                                Text("Send")
                            }
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun DialogKey(text: String, modifier: Modifier, onClick: () -> Unit) {
    OutlinedButton(
        onClick = onClick,
        modifier = modifier,
        shape = RoundedCornerShape(12.dp),
        border = BorderStroke(1.dp, BorderDark),
        contentPadding = PaddingValues(horizontal = 4.dp),
        colors = ButtonDefaults.outlinedButtonColors(contentColor = TextPrimary)
    ) {
        Text(text, maxLines = 1)
    }
}

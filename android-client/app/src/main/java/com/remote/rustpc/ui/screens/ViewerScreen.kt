package com.remote.rustpc.ui.screens

import android.graphics.Bitmap
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.Send
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
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
    onDisconnect: () -> Unit,
    modifier: Modifier = Modifier
) {
    var isDirectTouch by remember { mutableStateOf(true) }
    var scale by remember { mutableFloatStateOf(1f) }
    var offset by remember { mutableStateOf(Offset.Zero) }

    var showKeyboardDialog by remember { mutableStateOf(false) }
    var inputText by remember { mutableStateOf("") }

    Box(
        modifier = modifier
            .fillMaxSize()
            .background(BgDark)
    ) {
        // Fullscreen Remote Canvas
        RemoteCanvas(
            bitmap = bitmap,
            connection = connection,
            isDirectTouch = isDirectTouch,
            scale = scale,
            offset = offset,
            onTransform = { zoomChange, panChange ->
                scale = (scale * zoomChange).coerceIn(1f, 5f)
                if (scale <= 1.01f) {
                    offset = Offset.Zero
                } else {
                    offset += panChange
                }
            },
            modifier = Modifier.fillMaxSize()
        )

        // Waiting for first frame indicator
        if (bitmap == null) {
            Box(
                modifier = Modifier.fillMaxSize(),
                contentAlignment = Alignment.Center
            ) {
                Column(
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.spacedBy(16.dp)
                ) {
                    CircularProgressIndicator(color = AccentCyan)
                    Text(
                        text = "Receiving video stream...",
                        style = Typography.bodyMedium.copy(color = TextSecondary)
                    )
                }
            }
        }

        // Top Status HUD
        StatBadge(
            stats = stats,
            modifier = Modifier
                .align(Alignment.TopCenter)
                .statusBarsPadding()
                .padding(top = 12.dp)
        )

        // Bottom Floating Quick Toolbar
        QuickToolbar(
            connection = connection,
            isDirectTouch = isDirectTouch,
            onToggleTouchMode = { isDirectTouch = !isDirectTouch },
            onResetZoom = {
                scale = 1f
                offset = Offset.Zero
            },
            onOpenKeyboard = { showKeyboardDialog = true },
            onDisconnect = onDisconnect,
            modifier = Modifier
                .align(Alignment.BottomCenter)
                .navigationBarsPadding()
                .padding(bottom = 16.dp)
        )

        // Virtual Keyboard Input Dialog
        if (showKeyboardDialog) {
            Dialog(onDismissRequest = { showKeyboardDialog = false }) {
                Card(
                    modifier = Modifier.fillMaxWidth(),
                    shape = RoundedCornerShape(20.dp),
                    colors = CardDefaults.cardColors(containerColor = SurfaceDark)
                ) {
                    Column(
                        modifier = Modifier.padding(20.dp),
                        verticalArrangement = Arrangement.spacedBy(14.dp)
                    ) {
                        Text(
                            text = "Send Keyboard Text to PC",
                            style = Typography.titleLarge.copy(color = TextPrimary)
                        )

                        OutlinedTextField(
                            value = inputText,
                            onValueChange = { inputText = it },
                            placeholder = { Text("Type text to send...") },
                            colors = OutlinedTextFieldDefaults.colors(
                                focusedBorderColor = AccentCyan,
                                unfocusedBorderColor = BorderDark,
                                focusedTextColor = TextPrimary,
                                unfocusedTextColor = TextPrimary
                            ),
                            modifier = Modifier.fillMaxWidth()
                        )

                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.spacedBy(8.dp)
                        ) {
                            OutlinedButton(
                                onClick = {
                                    connection.sendKeyClick(KeyCode.Enter)
                                },
                                modifier = Modifier.weight(1f),
                                colors = ButtonDefaults.outlinedButtonColors(contentColor = TextPrimary)
                            ) {
                                Text("Enter ↵")
                            }

                            OutlinedButton(
                                onClick = {
                                    connection.sendKeyClick(KeyCode.Backspace)
                                },
                                modifier = Modifier.weight(1f),
                                colors = ButtonDefaults.outlinedButtonColors(contentColor = TextPrimary)
                            ) {
                                Text("Bksp ⌫")
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
                                Text("Cancel", color = TextSecondary)
                            }

                            Button(
                                onClick = {
                                    if (inputText.isNotEmpty()) {
                                        connection.sendText(inputText)
                                        inputText = ""
                                    }
                                    showKeyboardDialog = false
                                },
                                modifier = Modifier.weight(1f),
                                colors = ButtonDefaults.buttonColors(containerColor = AccentCyan, contentColor = BgDark)
                            ) {
                                Icon(Icons.AutoMirrored.Filled.Send, contentDescription = null)
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

package com.remote.rustpc.ui.screens

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.remote.rustpc.data.DEFAULT_PORT
import com.remote.rustpc.network.ConnectionState
import com.remote.rustpc.ui.theme.*

@Composable
fun ConnectScreen(
    connectionState: ConnectionState,
    initialHost: String,
    initialPort: Int,
    initialPassword: String,
    initialRemember: Boolean,
    recentHosts: List<String>,
    onRemoveRecent: (String) -> Unit,
    onConnect: (host: String, port: Int, pass: String, rememberPassword: Boolean) -> Unit,
    modifier: Modifier = Modifier
) {
    var host by remember { mutableStateOf(initialHost) }
    var portText by remember { mutableStateOf(initialPort.toString()) }
    var password by remember { mutableStateOf(initialPassword) }
    var showPassword by remember { mutableStateOf(false) }
    var rememberPw by remember { mutableStateOf(initialRemember) }

    val focus = LocalFocusManager.current
    val passwordFocus = remember { FocusRequester() }

    val isLoading = connectionState is ConnectionState.Connecting || connectionState is ConnectionState.Authenticating
    val serverError = (connectionState as? ConnectionState.Error)?.message

    val port = portText.toIntOrNull()
    val portValid = port != null && port in 1..65535
    val canConnect = !isLoading && host.isNotBlank() && portValid

    fun submit() {
        if (!canConnect) return
        focus.clearFocus()
        onConnect(host.trim(), port ?: DEFAULT_PORT, password, rememberPw)
    }

    Box(
        modifier = modifier
            .fillMaxSize()
            .background(BgDark)
            .systemBarsPadding()
            .imePadding(),
        contentAlignment = Alignment.TopCenter
    ) {
        Column(
            modifier = Modifier
                .widthIn(max = 480.dp)
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 20.dp, vertical = 28.dp),
            verticalArrangement = Arrangement.spacedBy(20.dp)
        ) {
            // ── Brand ──
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(12.dp)
            ) {
                Box(
                    modifier = Modifier
                        .size(44.dp)
                        .clip(RoundedCornerShape(12.dp))
                        .background(AccentIndigo),
                    contentAlignment = Alignment.Center
                ) {
                    Icon(
                        imageVector = Icons.Default.DesktopWindows,
                        contentDescription = null,
                        tint = TextPrimary,
                        modifier = Modifier.size(24.dp)
                    )
                }
                Column {
                    Text("Remote Desktop", style = Typography.titleLarge.copy(color = TextPrimary))
                    Text(
                        "View and control your PC, with sound",
                        style = Typography.labelMedium.copy(color = TextSecondary)
                    )
                }
            }

            Column {
                Text(
                    "Connect to a PC",
                    style = Typography.headlineMedium.copy(color = TextPrimary)
                )
                Spacer(Modifier.height(4.dp))
                Text(
                    "Enter the address and password shown on the host.",
                    style = Typography.bodyMedium.copy(color = TextSecondary)
                )
            }

            // ── Form card ──
            Surface(
                shape = RoundedCornerShape(16.dp),
                color = SurfaceDark,
                border = BorderStroke(1.dp, BorderDark)
            ) {
                Column(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(18.dp),
                    verticalArrangement = Arrangement.spacedBy(14.dp)
                ) {
                    Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                        AppTextField(
                            value = host,
                            onValueChange = { host = it.trim() },
                            label = "Address",
                            placeholder = "100.x.y.z",
                            leading = Icons.Default.Lan,
                            keyboardType = KeyboardType.Uri,
                            imeAction = ImeAction.Next,
                            enabled = !isLoading,
                            mono = true,
                            modifier = Modifier.weight(1f)
                        )
                        AppTextField(
                            value = portText,
                            onValueChange = { portText = it.filter(Char::isDigit).take(5) },
                            label = "Port",
                            placeholder = "$DEFAULT_PORT",
                            keyboardType = KeyboardType.Number,
                            imeAction = ImeAction.Next,
                            enabled = !isLoading,
                            mono = true,
                            isError = portText.isNotEmpty() && !portValid,
                            modifier = Modifier.width(104.dp)
                        )
                    }

                    AppTextField(
                        value = password,
                        onValueChange = { password = it },
                        label = "Password",
                        placeholder = "Host password",
                        leading = Icons.Default.Lock,
                        trailing = {
                            IconButton(onClick = { showPassword = !showPassword }) {
                                Icon(
                                    imageVector = if (showPassword) Icons.Default.Visibility else Icons.Default.VisibilityOff,
                                    contentDescription = if (showPassword) "Hide password" else "Show password",
                                    tint = TextSecondary
                                )
                            }
                        },
                        visualTransformation = if (showPassword) VisualTransformation.None else PasswordVisualTransformation(),
                        keyboardType = KeyboardType.Password,
                        imeAction = ImeAction.Go,
                        keyboardActions = KeyboardActions(onGo = { submit() }),
                        enabled = !isLoading,
                        modifier = Modifier
                            .fillMaxWidth()
                            .focusRequester(passwordFocus)
                    )

                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .clip(RoundedCornerShape(8.dp))
                            .clickable { rememberPw = !rememberPw }
                            .padding(vertical = 2.dp),
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Checkbox(
                            checked = rememberPw,
                            onCheckedChange = { rememberPw = it },
                            colors = CheckboxDefaults.colors(
                                checkedColor = AccentIndigo,
                                uncheckedColor = TextFaint,
                                checkmarkColor = TextPrimary
                            )
                        )
                        Column {
                            Text("Remember password", style = Typography.bodyMedium.copy(color = TextPrimary))
                            Text(
                                "Stored unencrypted on this phone",
                                style = Typography.labelMedium.copy(color = TextFaint)
                            )
                        }
                    }

                    if (serverError != null) {
                        Row(
                            modifier = Modifier
                                .fillMaxWidth()
                                .clip(RoundedCornerShape(10.dp))
                                .background(AccentRed.copy(alpha = 0.12f))
                                .border(1.dp, AccentRed.copy(alpha = 0.35f), RoundedCornerShape(10.dp))
                                .padding(12.dp),
                            verticalAlignment = Alignment.Top,
                            horizontalArrangement = Arrangement.spacedBy(10.dp)
                        ) {
                            Icon(
                                Icons.Default.ErrorOutline,
                                contentDescription = "Error",
                                tint = AccentRed,
                                modifier = Modifier.size(20.dp)
                            )
                            Text(
                                text = serverError,
                                style = Typography.bodyMedium.copy(color = AccentRed, fontSize = 13.sp)
                            )
                        }
                    }

                    Button(
                        onClick = { submit() },
                        enabled = canConnect,
                        shape = RoundedCornerShape(12.dp),
                        colors = ButtonDefaults.buttonColors(
                            containerColor = AccentIndigo,
                            contentColor = TextPrimary,
                            disabledContainerColor = SurfaceElevated,
                            disabledContentColor = TextFaint
                        ),
                        modifier = Modifier
                            .fillMaxWidth()
                            .height(52.dp)
                    ) {
                        if (isLoading) {
                            CircularProgressIndicator(
                                modifier = Modifier.size(20.dp),
                                color = TextPrimary,
                                strokeWidth = 2.dp
                            )
                            Spacer(Modifier.width(12.dp))
                            Text(
                                if (connectionState is ConnectionState.Authenticating) "Authenticating…" else "Connecting…",
                                fontWeight = FontWeight.SemiBold,
                                fontSize = 15.sp
                            )
                        } else {
                            Text("Connect", fontWeight = FontWeight.SemiBold, fontSize = 15.sp)
                        }
                    }
                }
            }

            // ── Recent hosts ──
            if (recentHosts.isNotEmpty()) {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(
                        "RECENT",
                        style = Typography.labelMedium.copy(color = TextFaint, letterSpacing = 1.sp)
                    )
                    recentHosts.forEach { rHost ->
                        Row(
                            modifier = Modifier
                                .fillMaxWidth()
                                .clip(RoundedCornerShape(12.dp))
                                .background(SurfaceDark)
                                .border(1.dp, BorderDark, RoundedCornerShape(12.dp))
                                .clickable(enabled = !isLoading) {
                                    host = rHost
                                    if (password.isEmpty()) passwordFocus.requestFocus()
                                }
                                .padding(start = 14.dp, top = 4.dp, bottom = 4.dp, end = 4.dp),
                            verticalAlignment = Alignment.CenterVertically
                        ) {
                            Icon(
                                Icons.Default.DesktopWindows,
                                contentDescription = null,
                                tint = TextSecondary,
                                modifier = Modifier.size(18.dp)
                            )
                            Spacer(Modifier.width(12.dp))
                            Text(
                                rHost,
                                style = Typography.bodyMedium.copy(
                                    color = TextPrimary,
                                    fontFamily = FontFamily.Monospace
                                ),
                                modifier = Modifier.weight(1f)
                            )
                            IconButton(onClick = { onRemoveRecent(rHost) }) {
                                Icon(
                                    Icons.Default.Close,
                                    contentDescription = "Remove $rHost",
                                    tint = TextFaint,
                                    modifier = Modifier.size(18.dp)
                                )
                            }
                        }
                    }
                }
            }

            // ── Help ──
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(12.dp))
                    .background(SurfaceDark.copy(alpha = 0.6f))
                    .padding(14.dp),
                verticalAlignment = Alignment.Top,
                horizontalArrangement = Arrangement.spacedBy(10.dp)
            ) {
                Icon(
                    imageVector = Icons.Default.Info,
                    contentDescription = null,
                    tint = TextSecondary,
                    modifier = Modifier.size(18.dp)
                )
                Text(
                    text = "Start sharing on the PC first. Both devices need to be on the same Wi-Fi or connected through Tailscale.",
                    style = Typography.bodyMedium.copy(color = TextSecondary, fontSize = 12.5.sp)
                )
            }
        }
    }
}

@Composable
private fun AppTextField(
    value: String,
    onValueChange: (String) -> Unit,
    label: String,
    placeholder: String,
    modifier: Modifier = Modifier,
    leading: androidx.compose.ui.graphics.vector.ImageVector? = null,
    trailing: (@Composable () -> Unit)? = null,
    keyboardType: KeyboardType = KeyboardType.Text,
    imeAction: ImeAction = ImeAction.Default,
    keyboardActions: KeyboardActions = KeyboardActions.Default,
    visualTransformation: VisualTransformation = VisualTransformation.None,
    enabled: Boolean = true,
    mono: Boolean = false,
    isError: Boolean = false
) {
    OutlinedTextField(
        value = value,
        onValueChange = onValueChange,
        label = { Text(label) },
        placeholder = { Text(placeholder, color = TextFaint) },
        leadingIcon = leading?.let { icon ->
            { Icon(icon, contentDescription = null, tint = TextSecondary) }
        },
        trailingIcon = trailing,
        singleLine = true,
        enabled = enabled,
        isError = isError,
        visualTransformation = visualTransformation,
        textStyle = Typography.bodyMedium.copy(
            fontFamily = if (mono) FontFamily.Monospace else FontFamily.Default
        ),
        keyboardOptions = KeyboardOptions(keyboardType = keyboardType, imeAction = imeAction),
        keyboardActions = keyboardActions,
        shape = RoundedCornerShape(12.dp),
        colors = OutlinedTextFieldDefaults.colors(
            focusedBorderColor = AccentIndigo,
            unfocusedBorderColor = BorderDark,
            errorBorderColor = AccentRed,
            focusedLabelColor = AccentIndigoLight,
            unfocusedLabelColor = TextSecondary,
            focusedTextColor = TextPrimary,
            unfocusedTextColor = TextPrimary,
            disabledTextColor = TextSecondary,
            disabledBorderColor = BorderDark,
            cursorColor = AccentIndigoLight,
            focusedContainerColor = Color0,
            unfocusedContainerColor = Color0
        ),
        modifier = modifier
    )
}

private val Color0 = androidx.compose.ui.graphics.Color.Transparent

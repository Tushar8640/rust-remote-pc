package com.remote.rustpc.ui.screens

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
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
import com.remote.rustpc.data.QualityMode
import com.remote.rustpc.network.ConnectionState
import com.remote.rustpc.network.Invite
import com.remote.rustpc.network.IrohBridge
import com.remote.rustpc.R
import androidx.compose.foundation.Image
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalLifecycleOwner
import androidx.compose.ui.res.painterResource
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import kotlinx.coroutines.delay
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
    initialQuality: QualityMode,
    onConnect: (host: String, port: Int, pass: String, rememberPassword: Boolean, quality: QualityMode) -> Unit,
    modifier: Modifier = Modifier
) {
    var host by remember { mutableStateOf(initialHost) }
    var portText by remember { mutableStateOf(initialPort.toString()) }
    var password by remember { mutableStateOf(initialPassword) }
    var showPassword by remember { mutableStateOf(false) }
    var rememberPw by remember { mutableStateOf(initialRemember) }
    var quality by remember { mutableStateOf(initialQuality) }
    var showOptions by remember { mutableStateOf(false) }

    val focus = LocalFocusManager.current

    val isLoading = connectionState is ConnectionState.Connecting || connectionState is ConnectionState.Authenticating
    val serverError = (connectionState as? ConnectionState.Error)?.message

    // What the field holds: an invite (everything included), a Pairing ID, or an address.
    val invite = remember(host) { if (IrohBridge.isInvite(host)) Invite.decode(host) else null }
    val inviteBroken = IrohBridge.isInvite(host) && invite == null
    val isPairingId = IrohBridge.isPairingId(host)
    val isAddress = host.isNotBlank() && invite == null && !inviteBroken && !isPairingId
    val port = portText.toIntOrNull()
    val portValid = !isAddress || (port != null && port in 1..65535)
    // The password is only asked for when the invite does not already carry it.
    val needsPassword = host.isNotBlank() && !inviteBroken && !(invite != null && invite.password.isNotEmpty())
    val canConnect = !isLoading && host.isNotBlank() && !inviteBroken && portValid

    // An invite on the clipboard is offered as a one-tap connect.
    val clipboard = LocalClipboardManager.current
    var clipInvite by remember { mutableStateOf<String?>(null) }
    var resumeTick by remember { mutableIntStateOf(0) }
    val lifecycleOwner = LocalLifecycleOwner.current
    DisposableEffect(lifecycleOwner) {
        val observer = LifecycleEventObserver { _, event ->
            if (event == Lifecycle.Event.ON_RESUME) resumeTick++
        }
        lifecycleOwner.lifecycle.addObserver(observer)
        onDispose { lifecycleOwner.lifecycle.removeObserver(observer) }
    }
    LaunchedEffect(resumeTick) {
        // Android only lets the focused app read the clipboard, so wait for focus.
        delay(350)
        val text = clipboard.getText()?.text?.trim()
        clipInvite = text?.takeIf { IrohBridge.isInvite(it) && Invite.decode(it) != null }
    }

    fun submit() {
        if (!canConnect) return
        focus.clearFocus()
        onConnect(host.trim(), port ?: DEFAULT_PORT, password, rememberPw, quality)
    }

    fun pasteFromClipboard() {
        val text = clipboard.getText()?.text?.trim()
        if (!text.isNullOrBlank()) host = text
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
                        .size(48.dp)
                        .clip(RoundedCornerShape(13.dp))
                        .background(
                            Brush.linearGradient(listOf(Color(0xFF818CF8), Color(0xFF4F46E5), Color(0xFF3730A3)))
                        ),
                    contentAlignment = Alignment.Center
                ) {
                    Image(
                        painter = painterResource(R.drawable.ic_launcher_foreground),
                        contentDescription = null,
                        modifier = Modifier.requiredSize(84.dp)
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
                    "Paste the invite you received from the PC you want to control.",
                    style = Typography.bodyMedium.copy(color = TextSecondary)
                )
            }

            // ── Invite on the clipboard ──
            if (host.isBlank() && clipInvite != null && !isLoading) {
                Surface(
                    shape = RoundedCornerShape(16.dp),
                    color = AccentGreen.copy(alpha = 0.12f),
                    border = BorderStroke(1.dp, AccentGreen.copy(alpha = 0.4f))
                ) {
                    Column(
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(16.dp),
                        verticalArrangement = Arrangement.spacedBy(10.dp)
                    ) {
                        Row(
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(10.dp)
                        ) {
                            Icon(Icons.Default.ContentPaste, contentDescription = null, tint = AccentGreen)
                            Column {
                                Text(
                                    "Invite found on your clipboard",
                                    style = Typography.bodyMedium.copy(color = TextPrimary, fontWeight = FontWeight.SemiBold)
                                )
                                Text(
                                    IrohBridge.shorten(clipInvite ?: ""),
                                    style = Typography.labelMedium.copy(color = TextSecondary, fontFamily = FontFamily.Monospace)
                                )
                            }
                        }
                        Button(
                            onClick = {
                                val clip = clipInvite ?: return@Button
                                host = clip
                                focus.clearFocus()
                                onConnect(clip, DEFAULT_PORT, "", rememberPw, quality)
                            },
                            shape = RoundedCornerShape(12.dp),
                            colors = ButtonDefaults.buttonColors(containerColor = AccentGreen, contentColor = BgDark),
                            modifier = Modifier
                                .fillMaxWidth()
                                .height(48.dp)
                        ) {
                            Text("Connect with it", fontWeight = FontWeight.SemiBold, fontSize = 15.sp)
                        }
                    }
                }
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
                    if (invite != null) {
                        // A pasted invite is shown as one tidy line instead of a wall of text.
                        Row(
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(12.dp)
                        ) {
                            Icon(Icons.Default.CheckCircle, contentDescription = null, tint = AccentGreen)
                            Column(modifier = Modifier.weight(1f)) {
                                Text(
                                    "Invite ready",
                                    style = Typography.bodyMedium.copy(color = TextPrimary, fontWeight = FontWeight.SemiBold)
                                )
                                Text(
                                    "…" + host.trim().takeLast(6) + if (invite.hasRelay) "  ·  Premium" else "",
                                    style = Typography.labelMedium.copy(color = TextSecondary, fontFamily = FontFamily.Monospace)
                                )
                            }
                            TextButton(onClick = { host = "" }, enabled = !isLoading) {
                                Text("Change", color = AccentIndigoLight)
                            }
                        }
                    } else {
                        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                            AppTextField(
                                value = host,
                                onValueChange = { host = it.trim() },
                                label = "Invite, Pairing ID or address",
                                placeholder = "Paste the invite here",
                                leading = Icons.Default.Link,
                                keyboardType = KeyboardType.Uri,
                                imeAction = ImeAction.Next,
                                enabled = !isLoading,
                                mono = true,
                                isError = inviteBroken,
                                modifier = Modifier.weight(1f)
                            )
                            // Port only matters for LAN/Tailscale addresses.
                            if (isAddress) {
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
                        }
                        if (inviteBroken) {
                            Text(
                                "That invite is not valid. Copy it again from the PC.",
                                style = Typography.labelMedium.copy(color = AccentRed)
                            )
                        }
                        if (host.isBlank()) {
                            OutlinedButton(
                                onClick = { pasteFromClipboard() },
                                enabled = !isLoading,
                                shape = RoundedCornerShape(12.dp),
                                border = BorderStroke(1.dp, AccentIndigo),
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .height(48.dp)
                            ) {
                                Icon(Icons.Default.ContentPaste, contentDescription = null, tint = AccentIndigoLight)
                                Spacer(Modifier.width(10.dp))
                                Text("Paste invite", color = AccentIndigoLight, fontWeight = FontWeight.SemiBold)
                            }
                        }
                    }

                    if (needsPassword) {
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
                            modifier = Modifier.fillMaxWidth()
                        )
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

                    // ── Options, out of the way ──
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .clip(RoundedCornerShape(8.dp))
                            .clickable { showOptions = !showOptions }
                            .padding(vertical = 4.dp),
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Text(
                            "Options",
                            style = Typography.labelMedium.copy(color = TextSecondary),
                            modifier = Modifier.weight(1f)
                        )
                        Icon(
                            if (showOptions) Icons.Default.ExpandLess else Icons.Default.ExpandMore,
                            contentDescription = null,
                            tint = TextSecondary
                        )
                    }
                    if (showOptions) {
                        QualitySelector(selected = quality, enabled = !isLoading) { quality = it }
                        if (needsPassword) {
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
                                .clickable(enabled = !isLoading) { host = rHost }
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
                                IrohBridge.shorten(rHost),
                                style = Typography.bodyMedium.copy(
                                    color = TextPrimary,
                                    fontFamily = FontFamily.Monospace
                                ),
                                modifier = Modifier.weight(1f)
                            )
                            IconButton(onClick = { onRemoveRecent(rHost) }) {
                                Icon(
                                    Icons.Default.Close,
                                    contentDescription = "Remove ${IrohBridge.shorten(rHost)}",
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
                    text = "On the PC, open the app, press Start sharing and Copy invite. Send it to this phone, then paste it here. It works from any network.",
                    style = Typography.bodyMedium.copy(color = TextSecondary, fontSize = 12.5.sp)
                )
            }
        }
    }
}

@Composable
private fun QualitySelector(
    selected: QualityMode,
    enabled: Boolean,
    onSelect: (QualityMode) -> Unit
) {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text("Video quality", style = Typography.labelMedium.copy(color = TextSecondary))
        Row(
            modifier = Modifier.horizontalScroll(rememberScrollState()),
            horizontalArrangement = Arrangement.spacedBy(6.dp)
        ) {
            QualityMode.entries.forEach { m ->
                ModeChip(m.label, m == selected, enabled) { onSelect(m) }
            }
        }
        Text(selected.hint, style = Typography.labelMedium.copy(color = TextFaint))
    }
}

@Composable
private fun ModeChip(label: String, selected: Boolean, enabled: Boolean, onClick: () -> Unit) {
    Surface(
        shape = RoundedCornerShape(10.dp),
        color = if (selected) AccentIndigo else SurfaceElevated,
        border = BorderStroke(1.dp, if (selected) AccentIndigoLight else BorderDark),
        modifier = Modifier
            .clip(RoundedCornerShape(10.dp))
            .clickable(enabled = enabled, onClick = onClick)
    ) {
        Text(
            label,
            modifier = Modifier.padding(horizontal = 12.dp, vertical = 8.dp),
            style = Typography.bodyMedium.copy(
                color = if (selected) TextPrimary else TextSecondary,
                fontSize = 13.sp
            )
        )
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

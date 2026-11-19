package com.remote.rustpc

import android.content.pm.ActivityInfo
import android.os.Bundle
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.Surface
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.core.splashscreen.SplashScreen.Companion.installSplashScreen
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import androidx.lifecycle.lifecycleScope
import com.remote.rustpc.data.ConnectionPreferences
import com.remote.rustpc.data.QualityMode
import com.remote.rustpc.network.ConnectionState
import com.remote.rustpc.network.IrohBridge
import com.remote.rustpc.network.RemoteConnection
import com.remote.rustpc.ui.screens.ConnectScreen
import com.remote.rustpc.ui.screens.ViewerScreen
import com.remote.rustpc.ui.theme.BgDark
import com.remote.rustpc.ui.theme.RustRemotePCTheme

class MainActivity : ComponentActivity() {

    private lateinit var connection: RemoteConnection
    private lateinit var prefs: ConnectionPreferences

    override fun onCreate(savedInstanceState: Bundle?) {
        // Must come before super.onCreate: swaps the splash theme for the app theme.
        installSplashScreen()
        super.onCreate(savedInstanceState)

        // Draw behind the system bars; screens add their own insets.
        WindowCompat.setDecorFitsSystemWindows(window, false)

        prefs = ConnectionPreferences(this)
        connection = RemoteConnection(lifecycleScope)

        setContent {
            RustRemotePCTheme {
                Surface(
                    modifier = Modifier.fillMaxSize(),
                    color = BgDark
                ) {
                    val connectionState by connection.connectionState.collectAsState()
                    val bitmap by connection.latestBitmap.collectAsState()
                    val stats by connection.stats.collectAsState()
                    val audioMuted by connection.audioMuted.collectAsState()
                    val audioVolume by connection.audioVolume.collectAsState()
                    val audioActive by connection.audioActive.collectAsState()

                    var fullscreen by remember { mutableStateOf(true) }
                    var landscapeLock by remember { mutableStateOf(false) }
                    var recents by remember { mutableStateOf(prefs.getRecentHosts()) }
                    var rememberPwState by remember { mutableStateOf(prefs.rememberPassword) }

                    val connected = connectionState is ConnectionState.Connected

                    // Keep screen on while connected to PC
                    LaunchedEffect(connected) {
                        if (connected) {
                            window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
                        } else {
                            window.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
                        }
                    }

                    // Immersive full screen (hides status + navigation bars) while viewing the PC.
                    LaunchedEffect(connected, fullscreen) {
                        applyFullscreen(connected && fullscreen)
                    }

                    LaunchedEffect(connected, landscapeLock) {
                        requestedOrientation = if (connected && landscapeLock) {
                            ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE
                        } else {
                            ActivityInfo.SCREEN_ORIENTATION_UNSPECIFIED
                        }
                    }

                    when (connectionState) {
                        is ConnectionState.Connected -> {
                            BackHandler {
                                connection.disconnect()
                            }

                            ViewerScreen(
                                bitmap = bitmap,
                                stats = stats,
                                connection = connection,
                                audioMuted = audioMuted,
                                audioVolume = audioVolume,
                                audioActive = audioActive,
                                isFullscreen = fullscreen,
                                onToggleFullscreen = { fullscreen = !fullscreen },
                                isLandscapeLocked = landscapeLock,
                                onToggleRotation = { landscapeLock = !landscapeLock },
                                onDisconnect = {
                                    connection.disconnect()
                                },
                                hudOpacity = prefs.hudOpacity,
                                hudOffset = androidx.compose.ui.geometry.Offset(prefs.hudOffsetX, prefs.hudOffsetY),
                                onHudChange = { alpha, pos ->
                                    prefs.hudOpacity = alpha
                                    prefs.hudOffsetX = pos.x
                                    prefs.hudOffsetY = pos.y
                                }
                            )
                        }
                        else -> {
                            ConnectScreen(
                                connectionState = connectionState,
                                initialHost = prefs.lastHost,
                                initialPort = prefs.lastPort,
                                initialPassword = prefs.lastPassword,
                                initialRemember = rememberPwState,
                                recentHosts = recents,
                                onRemoveRecent = {
                                    prefs.removeRecentHost(it)
                                    recents = prefs.getRecentHosts()
                                },
                                initialQuality = QualityMode.fromId(prefs.qualityModeId),
                                onConnect = { host, port, pass, rememberPw, quality ->
                                    prefs.rememberPassword = rememberPw
                                    rememberPwState = rememberPw
                                    if (!IrohBridge.isInvite(host)) prefs.lastHost = host
                                    prefs.lastPort = port
                                    prefs.lastPassword = pass
                                    prefs.qualityModeId = quality.id
                                    // Join codes expire within a day, so do not keep them in the recent list.
                                    if (!IrohBridge.isInvite(host)) prefs.addRecentHost(host)
                                    recents = prefs.getRecentHosts()
                                    connection.connect(
                                        host, port, pass,
                                        quality = quality
                                    )
                                }
                            )
                        }
                    }
                }
            }
        }
    }

    private fun applyFullscreen(on: Boolean) {
        val controller = WindowInsetsControllerCompat(window, window.decorView)
        controller.systemBarsBehavior =
            WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
        if (on) {
            controller.hide(WindowInsetsCompat.Type.systemBars())
        } else {
            controller.show(WindowInsetsCompat.Type.systemBars())
        }
    }

    override fun onDestroy() {
        super.onDestroy()
        connection.disconnect()
    }
}

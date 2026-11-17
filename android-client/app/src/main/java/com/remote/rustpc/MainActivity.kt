package com.remote.rustpc

import android.os.Bundle
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.Surface
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.lifecycle.lifecycleScope
import com.remote.rustpc.data.ConnectionPreferences
import com.remote.rustpc.network.ConnectionState
import com.remote.rustpc.network.RemoteConnection
import com.remote.rustpc.ui.screens.ConnectScreen
import com.remote.rustpc.ui.screens.ViewerScreen
import com.remote.rustpc.ui.theme.BgDark
import com.remote.rustpc.ui.theme.RustRemotePCTheme

class MainActivity : ComponentActivity() {

    private lateinit var connection: RemoteConnection
    private lateinit var prefs: ConnectionPreferences

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

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

                    // Keep screen on while connected to PC
                    LaunchedEffect(connectionState) {
                        if (connectionState is ConnectionState.Connected) {
                            window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
                        } else {
                            window.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
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
                                onDisconnect = {
                                    connection.disconnect()
                                }
                            )
                        }
                        else -> {
                            ConnectScreen(
                                connectionState = connectionState,
                                initialHost = prefs.lastHost,
                                initialPort = prefs.lastPort,
                                initialPassword = prefs.lastPassword,
                                recentHosts = prefs.getRecentHosts(),
                                onConnect = { host, port, pass ->
                                    prefs.lastHost = host
                                    prefs.lastPort = port
                                    prefs.lastPassword = pass
                                    prefs.addRecentHost(host)
                                    connection.connect(host, port, pass)
                                }
                            )
                        }
                    }
                }
            }
        }
    }

    override fun onDestroy() {
        super.onDestroy()
        connection.disconnect()
    }
}

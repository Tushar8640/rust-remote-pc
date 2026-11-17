package com.remote.rustpc.data

import android.content.Context
import android.content.SharedPreferences

data class SavedConnection(
    val host: String,
    val port: Int,
    val label: String
)

class ConnectionPreferences(context: Context) {
    private val prefs: SharedPreferences =
        context.getSharedPreferences("rust_remote_pc_prefs", Context.MODE_PRIVATE)

    var lastHost: String
        get() = prefs.getString("last_host", "100.100.100.100") ?: "100.100.100.100"
        set(value) = prefs.edit().putString("last_host", value).apply()

    var lastPort: Int
        get() = prefs.getInt("last_port", DEFAULT_PORT)
        set(value) = prefs.edit().putInt("last_port", value).apply()

    var lastPassword: String
        get() = prefs.getString("last_password", "") ?: ""
        set(value) = prefs.edit().putString("last_password", value).apply()

    fun getRecentHosts(): List<String> {
        val raw = prefs.getString("recent_hosts", "") ?: ""
        return if (raw.isBlank()) emptyList() else raw.split(",").filter { it.isNotBlank() }
    }

    fun addRecentHost(host: String) {
        val list = getRecentHosts().toMutableList()
        list.remove(host)
        list.add(0, host)
        val trimmed = list.take(5)
        prefs.edit().putString("recent_hosts", trimmed.joinToString(",")).apply()
    }
}

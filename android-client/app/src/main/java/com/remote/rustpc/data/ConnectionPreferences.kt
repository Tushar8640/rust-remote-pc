package com.remote.rustpc.data

import android.content.Context
import android.content.SharedPreferences

class ConnectionPreferences(context: Context) {
    private val prefs: SharedPreferences =
        context.getSharedPreferences("rust_remote_pc_prefs", Context.MODE_PRIVATE)

    var lastHost: String
        get() = prefs.getString("last_host", "") ?: ""
        set(value) = prefs.edit().putString("last_host", value).apply()

    var lastPort: Int
        get() = prefs.getInt("last_port", DEFAULT_PORT)
        set(value) = prefs.edit().putInt("last_port", value).apply()

    /** The password is only stored when the user opts in. Off by default. */
    var rememberPassword: Boolean
        get() = prefs.getBoolean("remember_password", false)
        set(value) {
            val e = prefs.edit().putBoolean("remember_password", value)
            if (!value) e.remove("last_password")
            e.apply()
        }

    var lastPassword: String
        get() = if (rememberPassword) prefs.getString("last_password", "") ?: "" else ""
        set(value) {
            if (rememberPassword) prefs.edit().putString("last_password", value).apply()
        }

    /** Last chosen video quality mode (wire id). */
    var qualityModeId: Int
        get() = prefs.getInt("quality_mode", 0)
        set(value) = prefs.edit().putInt("quality_mode", value).apply()

    /** Look of the draggable ping/fps badge in the viewer. */
    var hudOpacity: Float
        get() = prefs.getFloat("hud_opacity", 0.9f)
        set(value) = prefs.edit().putFloat("hud_opacity", value).apply()

    /** Badge position as a pixel offset from its default spot (top centre). */
    var hudOffsetX: Float
        get() = prefs.getFloat("hud_x", 0f)
        set(value) = prefs.edit().putFloat("hud_x", value).apply()

    var hudOffsetY: Float
        get() = prefs.getFloat("hud_y", 0f)
        set(value) = prefs.edit().putFloat("hud_y", value).apply()

    fun getRecentHosts(): List<String> {
        val raw = prefs.getString("recent_hosts", "") ?: ""
        return if (raw.isBlank()) emptyList() else raw.split(",").filter { it.isNotBlank() }
    }

    fun addRecentHost(host: String) {
        val list = getRecentHosts().toMutableList()
        list.remove(host)
        list.add(0, host)
        prefs.edit().putString("recent_hosts", list.take(5).joinToString(",")).apply()
    }

    fun removeRecentHost(host: String) {
        val list = getRecentHosts().filter { it != host }
        prefs.edit().putString("recent_hosts", list.joinToString(",")).apply()
    }
}

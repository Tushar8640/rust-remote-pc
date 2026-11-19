package com.remote.rustpc.network

import android.util.Base64

/**
 * A premium join code shared by a host: `rrpc1.` + base64url(`pairingId|relayUrl|pass`).
 *
 * It tells the phone which PC to reach, which premium relay to meet on, and carries a
 * short-lived pass for that relay, so the phone needs no relay settings of its own.
 * Matches `premium_auth::JoinCode` on the PC side.
 */
data class JoinCode(val pairingId: String, val relayUrl: String, val pass: String) {

    companion object {
        private const val PREFIX = "rrpc1."

        fun looksLikeJoinCode(s: String): Boolean = s.trim().startsWith(PREFIX)

        /** Returns null if [s] is not a valid join code. */
        fun decode(s: String): JoinCode? {
            val body = s.trim().removePrefix(PREFIX)
            if (body.length == s.trim().length) return null
            val raw = try {
                String(
                    Base64.decode(body, Base64.URL_SAFE or Base64.NO_PADDING or Base64.NO_WRAP),
                    Charsets.UTF_8
                )
            } catch (e: IllegalArgumentException) {
                return null
            }
            val parts = raw.split('|')
            if (parts.size != 3) return null
            val (id, url, pass) = parts
            val idOk = id.length == 64 && id.all { it in '0'..'9' || it in 'a'..'f' || it in 'A'..'F' }
            if (!idOk || url.isEmpty() || pass.isEmpty()) return null
            return JoinCode(id.lowercase(), url, pass)
        }
    }
}

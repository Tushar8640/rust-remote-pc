package com.remote.rustpc.network

import android.util.Base64

/**
 * An invite copied from a host PC. One string holds everything needed to connect:
 * which PC, its password and, for a premium host, the relay to meet on plus a pass.
 *
 * Format `rrpc2.` + base64url(`id \n password \n relayUrl \n pass`). Older invites
 * (`rrpc1.` + base64url(`id|relayUrl|pass`)) have no password. Matches
 * `premium_auth::Invite` on the PC side.
 */
data class Invite(
    val pairingId: String,
    val password: String,
    val relayUrl: String,
    val pass: String
) {
    val hasRelay: Boolean get() = relayUrl.isNotEmpty()

    companion object {
        private const val V2 = "rrpc2."
        private const val V1 = "rrpc1."

        fun looksLikeInvite(s: String): Boolean {
            val t = s.trim()
            return t.startsWith(V2) || t.startsWith(V1)
        }

        /** Returns null if [s] is not a valid invite. */
        fun decode(s: String): Invite? {
            val t = s.trim()
            val legacy = !t.startsWith(V2)
            val body = when {
                t.startsWith(V2) -> t.removePrefix(V2)
                t.startsWith(V1) -> t.removePrefix(V1)
                else -> return null
            }
            val raw = try {
                String(
                    Base64.decode(body, Base64.URL_SAFE or Base64.NO_PADDING or Base64.NO_WRAP),
                    Charsets.UTF_8
                )
            } catch (e: IllegalArgumentException) {
                return null
            }
            val parts = raw.split(if (legacy) '|' else '\n')
            val (id, password, url, pass) = if (legacy) {
                if (parts.size != 3) return null
                listOf(parts[0], "", parts[1], parts[2])
            } else {
                if (parts.size != 4) return null
                parts
            }
            val idOk = id.length == 64 && id.all { it in '0'..'9' || it in 'a'..'f' || it in 'A'..'F' }
            if (!idOk) return null
            if (url.isEmpty() != pass.isEmpty()) return null
            return Invite(id.lowercase(), password, url, pass)
        }
    }
}

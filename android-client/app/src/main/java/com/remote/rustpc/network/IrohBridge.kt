package com.remote.rustpc.network

/**
 * Peer-to-peer connectivity via the native `remote_bridge` library (Rust, iroh).
 *
 * The native side dials the PC by its Pairing ID (hole-punching across NAT with
 * relay fallback, end-to-end encrypted) and exposes the stream on a loopback TCP
 * port. The regular socket code then connects to 127.0.0.1:[Endpoint.port] and
 * must send [Endpoint.secret] (ASCII) as its very first bytes.
 */
object IrohBridge {

    class Endpoint(val port: Int, val secret: String)

    private val libraryError: String? = try {
        System.loadLibrary("remote_bridge")
        null
    } catch (e: UnsatisfiedLinkError) {
        e.message ?: "native library missing"
    }

    /** True if [s] looks like a Pairing ID (64 hex characters) rather than a host/IP. */
    fun isPairingId(s: String): Boolean {
        val t = s.trim()
        return t.length == 64 && t.all { it in '0'..'9' || it in 'a'..'f' || it in 'A'..'F' }
    }

    /** True if [s] is an invite copied from a host PC. */
    fun isInvite(s: String): Boolean = Invite.looksLikeInvite(s)

    /** Short form for display, e.g. `b6a1fada…7536fb2` or `Invite …AbCdEf`. */
    fun shorten(s: String): String = when {
        isPairingId(s) -> "${s.take(8)}…${s.takeLast(6)}"
        isInvite(s) -> "Invite …${s.trim().takeLast(6)}"
        else -> s
    }

    /**
     * Dials the PC and starts the loopback bridge. Blocking: call from a background
     * dispatcher. Throws [IllegalStateException] with a user-readable message.
     */
    fun connect(pairingId: String, relayUrl: String = "", relayKey: String = ""): Endpoint {
        libraryError?.let { error("P2P support is unavailable on this device ($it).") }
        val reply = nativeConnect(pairingId.trim().lowercase(), relayUrl.trim(), relayKey.trim())
        if (reply.startsWith("OK ")) {
            val parts = reply.split(' ')
            val port = parts.getOrNull(1)?.toIntOrNull()
            val secret = parts.getOrNull(2)
            if (port != null && secret != null) return Endpoint(port, secret)
        }
        error(reply.removePrefix("ERR ").ifBlank { "Could not reach the PC." })
    }

    /** `relayUrl` and `relayKey` select a self-hosted premium relay; both empty means the public relays. */
    private external fun nativeConnect(pairingId: String, relayUrl: String, relayKey: String): String
}

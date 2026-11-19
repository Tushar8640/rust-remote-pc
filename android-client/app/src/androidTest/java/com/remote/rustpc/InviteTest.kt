package com.remote.rustpc

import androidx.test.ext.junit.runners.AndroidJUnit4
import com.remote.rustpc.network.Invite
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Checks the Kotlin invite decoder against invites built the way the PC app builds them
 * (premium_auth::Invite). The strings use a made-up Pairing ID and a made-up relay pass.
 */
@RunWith(AndroidJUnit4::class)
class InviteTest {
    private val id = "ab".repeat(32)

    // rrpc2.  id \n "Pw demo|1 é" \n (no relay) \n (no pass)
    private val standard = "rrpc2.YWJhYmFiYWJhYmFiYWJhYmFiYWJhYmFiYWJhYmFiYWJhYmFiYWJhYmFiYWJhYmFiYWJhYmFiYWJhYmFiYWJhYgpQdyBkZW1vfDEgw6kKCg"

    // rrpc2.  id \n secret \n https://relay.example \n acme.c.1800000000.AAAAAAAAAAAAAAAAAAAAAA
    private val premium = "rrpc2.YWJhYmFiYWJhYmFiYWJhYmFiYWJhYmFiYWJhYmFiYWJhYmFiYWJhYmFiYWJhYmFiYWJhYmFiYWJhYmFiYWJhYgpzZWNyZXQKaHR0cHM6Ly9yZWxheS5leGFtcGxlCmFjbWUuYy4xODAwMDAwMDAwLkFBQUFBQUFBQUFBQUFBQUFBQUFBQUE"

    // older rrpc1.  id | https://relay.example | pass   (no password)
    private val legacy = "rrpc1.YWJhYmFiYWJhYmFiYWJhYmFiYWJhYmFiYWJhYmFiYWJhYmFiYWJhYmFiYWJhYmFiYWJhYmFiYWJhYmFiYWJhYnxodHRwczovL3JlbGF5LmV4YW1wbGV8YWNtZS5jLjE4MDAwMDAwMDAuQUFBQUFBQUFBQUFBQUFBQUFBQUFBQQ"

    // relay URL without a pass: not a valid invite
    private val broken = "rrpc2.YWJhYmFiYWJhYmFiYWJhYmFiYWJhYmFiYWJhYmFiYWJhYmFiYWJhYmFiYWJhYmFiYWJhYmFiYWJhYmFiYWJhYgpzZWNyZXQKaHR0cHM6Ly9yZWxheS5leGFtcGxlCg"

    @Test
    fun standardInviteCarriesIdAndPasswordOnly() {
        val inv = Invite.decode(standard)
        assertNotNull(inv)
        assertEquals(id, inv!!.pairingId)
        assertEquals("Pw demo|1 é", inv.password)
        assertFalse(inv.hasRelay)
    }

    @Test
    fun premiumInviteCarriesRelayAndPass() {
        val inv = Invite.decode("  $premium\n")
        assertNotNull(inv)
        assertEquals("secret", inv!!.password)
        assertTrue(inv.hasRelay)
        assertEquals("https://relay.example", inv.relayUrl)
        assertEquals("acme.c.1800000000.AAAAAAAAAAAAAAAAAAAAAA", inv.pass)
    }

    @Test
    fun olderInvitesStillDecodeWithoutAPassword() {
        val inv = Invite.decode(legacy)
        assertNotNull(inv)
        assertEquals("", inv!!.password)
        assertTrue(inv.hasRelay)
    }

    @Test
    fun junkIsRejected() {
        assertNull(Invite.decode("nope"))
        assertNull(Invite.decode("rrpc2.!!!"))
        assertNull(Invite.decode(broken))
        assertTrue(Invite.looksLikeInvite(standard))
        assertFalse(Invite.looksLikeInvite(id))
    }
}

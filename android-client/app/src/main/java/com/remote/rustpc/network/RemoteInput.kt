package com.remote.rustpc.network

import com.remote.rustpc.data.MouseBtn

/**
 * The mouse input the touch canvas can send to the PC. [RemoteConnection] implements it;
 * tests use a recording fake.
 *
 * Scroll values are mouse-wheel units: 120 is one notch, positive `deltaY` scrolls up
 * (content moves down), positive `deltaX` scrolls right.
 */
interface RemoteInput {
    fun sendMouseMove(xRatio: Float, yRatio: Float)
    fun sendMouseDown(button: MouseBtn)
    fun sendMouseUp(button: MouseBtn)
    fun sendMouseClick(button: MouseBtn)
    fun sendMouseScroll(deltaX: Int, deltaY: Int)
}

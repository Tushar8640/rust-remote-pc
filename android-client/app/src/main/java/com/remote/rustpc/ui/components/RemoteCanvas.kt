package com.remote.rustpc.ui.components

import android.graphics.Bitmap
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.calculateCentroidSize
import androidx.compose.foundation.gestures.calculatePan
import androidx.compose.foundation.gestures.calculateZoom
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import com.remote.rustpc.data.MouseBtn
import com.remote.rustpc.network.RemoteInput
import kotlin.math.abs
import kotlin.math.hypot
import kotlin.math.roundToInt

private const val LONG_PRESS_MS = 550L
private const val DOUBLE_TAP_MS = 350L
private const val WHEEL_NOTCH = 120f        // one mouse-wheel notch, as the PC expects it
private const val MIN_WHEEL_UNITS = 12f     // smallest scroll packet, keeps traffic modest
private const val MODE_UNDECIDED = 0
private const val MODE_SCROLL = 1
private const val MODE_ZOOM = 2

/**
 * Remote screen with touch input.
 *
 * Both modes:
 *   one tap = one click, two quick taps = double click, long press = right click,
 *   two-finger drag = scroll (vertical and horizontal), two-finger pinch = zoom.
 * Direct touch: dragging one finger drags with the left button held.
 * Trackpad:     dragging one finger moves the cursor.
 */
@Composable
fun RemoteCanvas(
    bitmap: Bitmap?,
    connection: RemoteInput,
    isDirectTouch: Boolean,
    scale: Float,
    offset: Offset,
    onTransform: (zoomChange: Float, panChange: Offset) -> Unit,
    modifier: Modifier = Modifier
) {
    var canvasSize by remember { mutableStateOf(IntSize.Zero) }

    // Virtual cursor for trackpad mode
    var cursorRatioX by remember { mutableFloatStateOf(0.5f) }
    var cursorRatioY by remember { mutableFloatStateOf(0.5f) }

    // The gesture loop is started once; it always reads the latest values through these.
    val directState by rememberUpdatedState(isDirectTouch)
    val bitmapState by rememberUpdatedState(bitmap)
    val scaleState by rememberUpdatedState(scale)
    val offsetState by rememberUpdatedState(offset)
    val transformState by rememberUpdatedState(onTransform)

    Box(
        modifier = modifier
            .fillMaxSize()
            .pointerInput(Unit) {
                val slop = viewConfiguration.touchSlop * 1.5f
                // Two taps closer than this count as a double click (about 5 mm on screen).
                val doubleTapSlop = 32.dp.toPx()
                // A two-finger gesture is a pinch only if the fingers spread or close by this much.
                val pinchSlop = 10.dp.toPx()
                // Finger travel that equals one mouse-wheel notch. About 1:1 with the page
                // when the remote screen is shown fit-to-screen.
                val wheelUnitsPerPx = WHEEL_NOTCH / 22.dp.toPx()

                awaitPointerEventScope {
                    var downTime = 0L
                    var downPos = Offset.Zero
                    var dragging = false     // left button currently held on the host
                    var multi = false        // a multi-finger gesture is in progress
                    var lastTapTime = 0L
                    var lastTapPos = Offset.Zero
                    var lastTapRatio: Pair<Float, Float>? = null

                    // Two-finger gesture state
                    var mode = MODE_UNDECIDED
                    var startSize = 0f
                    var movedPan = Offset.Zero
                    var wheelX = 0f
                    var wheelY = 0f

                    fun ratioOf(p: Offset) =
                        calculateScreenRatio(p, canvasSize, bitmapState, scaleState, offsetState)

                    fun inside(r: Pair<Float, Float>) = r.first in 0f..1f && r.second in 0f..1f

                    /** Turns finger movement into mouse-wheel movement (natural scrolling). */
                    fun scrollBy(panPx: Offset) {
                        wheelY += panPx.y * wheelUnitsPerPx      // fingers down = page up
                        wheelX -= panPx.x * wheelUnitsPerPx      // fingers left = page right
                        val dy = if (abs(wheelY) >= MIN_WHEEL_UNITS) wheelY.toInt() else 0
                        val dx = if (abs(wheelX) >= MIN_WHEEL_UNITS) wheelX.toInt() else 0
                        if (dx != 0 || dy != 0) {
                            connection.sendMouseScroll(dx, dy)
                            wheelY -= dy
                            wheelX -= dx
                        }
                    }

                    while (true) {
                        val event = awaitPointerEvent()
                        val changes = event.changes
                        if (changes.isEmpty()) continue
                        val pressedCount = changes.count { it.pressed }

                        // ── Two or more fingers: scroll, or pinch to zoom ──
                        if (pressedCount >= 2) {
                            if (!multi) {
                                multi = true
                                if (dragging) {
                                    connection.sendMouseUp(MouseBtn.LEFT)
                                    dragging = false
                                }
                                mode = MODE_UNDECIDED
                                // The size is only meaningful once both fingers have been seen moving
                                // (the event that adds the second finger reports just one), so it is
                                // captured on the next event.
                                startSize = -1f
                                movedPan = Offset.Zero
                                wheelX = 0f
                                wheelY = 0f
                                lastTapTime = 0L // fingers on screen cancel a pending double click
                            }
                            val zoom = event.calculateZoom()
                            val pan = event.calculatePan()

                            // Decide once, so a scroll never turns into a zoom half way through.
                            if (mode == MODE_UNDECIDED) {
                                movedPan += pan
                                val size = event.calculateCentroidSize()
                                if (startSize < 0f) {
                                    if (size > 0f) startSize = size
                                } else if (abs(size - startSize) > pinchSlop) {
                                    mode = MODE_ZOOM
                                } else if (hypot(movedPan.x.toDouble(), movedPan.y.toDouble()) > slop) {
                                    mode = MODE_SCROLL
                                    if (directState) {
                                        // The wheel acts on the window under the fingers.
                                        val centre = changes.filter { it.pressed }
                                            .map { it.position }
                                            .reduce { a, b -> a + b } / pressedCount.toFloat()
                                        val r = ratioOf(centre)
                                        if (inside(r)) connection.sendMouseMove(r.first, r.second)
                                    }
                                    scrollBy(movedPan) // catch up on the movement spent deciding
                                }
                            } else if (mode == MODE_SCROLL) {
                                scrollBy(pan)
                            } else if (zoom != 1f || pan != Offset.Zero) {
                                transformState(zoom, pan)
                            }
                            changes.forEach { it.consume() }
                            continue
                        }

                        // After a multi-finger gesture, ignore the remaining finger until all lift.
                        if (multi) {
                            if (pressedCount == 0) multi = false
                            changes.forEach { it.consume() }
                            continue
                        }

                        val change = changes[0]

                        if (change.pressed && !change.previousPressed) {
                            // Finger down
                            downTime = System.currentTimeMillis()
                            downPos = change.position
                            dragging = false
                            if (directState) {
                                val r = ratioOf(downPos)
                                if (inside(r)) connection.sendMouseMove(r.first, r.second)
                            }
                            change.consume()
                        } else if (change.pressed && change.previousPressed) {
                            // Finger moved
                            val dist = hypot(
                                (change.position.x - downPos.x).toDouble(),
                                (change.position.y - downPos.y).toDouble()
                            ).toFloat()

                            if (directState) {
                                if (!dragging && dist > slop) {
                                    // Begin a left-button drag where the finger first touched.
                                    val start = ratioOf(downPos)
                                    if (inside(start)) {
                                        connection.sendMouseMove(start.first, start.second)
                                        connection.sendMouseDown(MouseBtn.LEFT)
                                        dragging = true
                                    }
                                }
                                if (dragging) {
                                    val r = ratioOf(change.position)
                                    connection.sendMouseMove(
                                        r.first.coerceIn(0f, 1f),
                                        r.second.coerceIn(0f, 1f)
                                    )
                                }
                            } else if (dist > slop || dragging) {
                                // Trackpad: relative cursor movement
                                dragging = true // reused as "moved beyond slop" flag
                                val pan = change.position - change.previousPosition
                                val sensitivity = 0.0018f / scaleState.coerceAtLeast(1f)
                                cursorRatioX = (cursorRatioX + pan.x * sensitivity).coerceIn(0f, 1f)
                                cursorRatioY = (cursorRatioY + pan.y * sensitivity).coerceIn(0f, 1f)
                                connection.sendMouseMove(cursorRatioX, cursorRatioY)
                            }
                            change.consume()
                        } else if (!change.pressed && change.previousPressed) {
                            // Finger up
                            val now = System.currentTimeMillis()
                            val elapsed = now - downTime
                            val moved = dragging

                            if (directState && moved) {
                                connection.sendMouseUp(MouseBtn.LEFT)
                            } else if (!moved) {
                                // A tap (no meaningful movement)
                                val tapRatio = if (directState) ratioOf(downPos) else null
                                val canClick = tapRatio?.let { inside(it) } ?: true

                                if (canClick) {
                                    if (elapsed >= LONG_PRESS_MS) {
                                        tapRatio?.let { connection.sendMouseMove(it.first, it.second) }
                                        connection.sendMouseClick(MouseBtn.RIGHT)
                                        lastTapTime = 0L
                                    } else {
                                        // One tap is one click. A second tap soon after, in about
                                        // the same place, is a second click: Windows turns that
                                        // pair into a double click. The second click is sent at
                                        // exactly the first click's position, because Windows
                                        // only pairs clicks that land within a few pixels.
                                        val isSecond = now - lastTapTime < DOUBLE_TAP_MS &&
                                            hypot(
                                                (downPos.x - lastTapPos.x).toDouble(),
                                                (downPos.y - lastTapPos.y).toDouble()
                                            ) < doubleTapSlop
                                        val at = if (isSecond && lastTapRatio != null) lastTapRatio else tapRatio
                                        at?.let { connection.sendMouseMove(it.first, it.second) }
                                        connection.sendMouseClick(MouseBtn.LEFT)
                                        if (isSecond) {
                                            lastTapTime = 0L
                                        } else {
                                            lastTapTime = now
                                            lastTapPos = downPos
                                            lastTapRatio = tapRatio
                                        }
                                    }
                                }
                            }
                            dragging = false
                            change.consume()
                        }
                    }
                }
            }
    ) {
        Canvas(modifier = Modifier.fillMaxSize()) {
            canvasSize = IntSize(size.width.roundToInt(), size.height.roundToInt())

            if (bitmap != null && !bitmap.isRecycled) {
                val imageBitmap = bitmap.asImageBitmap()

                val viewWidth = size.width
                val viewHeight = size.height
                val imgWidth = bitmap.width.toFloat()
                val imgHeight = bitmap.height.toFloat()

                val scaleFactor = minOf(viewWidth / imgWidth, viewHeight / imgHeight)
                val baseDstWidth = imgWidth * scaleFactor
                val baseDstHeight = imgHeight * scaleFactor

                val baseOffsetX = (viewWidth - baseDstWidth) / 2f
                val baseOffsetY = (viewHeight - baseDstHeight) / 2f

                val finalWidth = baseDstWidth * scale
                val finalHeight = baseDstHeight * scale
                val finalX = baseOffsetX + offset.x - (finalWidth - baseDstWidth) / 2f
                val finalY = baseOffsetY + offset.y - (finalHeight - baseDstHeight) / 2f

                drawImage(
                    image = imageBitmap,
                    dstOffset = IntOffset(finalX.roundToInt(), finalY.roundToInt()),
                    dstSize = IntSize(finalWidth.roundToInt(), finalHeight.roundToInt())
                )

                // Trackpad mode: draw a cursor pointer
                if (!isDirectTouch) {
                    val cx = finalX + cursorRatioX * finalWidth
                    val cy = finalY + cursorRatioY * finalHeight

                    val cursorPath = Path().apply {
                        moveTo(cx, cy)
                        lineTo(cx + 18f, cy + 14f)
                        lineTo(cx + 8f, cy + 16f)
                        lineTo(cx + 5f, cy + 24f)
                        close()
                    }
                    drawPath(cursorPath, Color.White)
                    drawPath(cursorPath, Color.Black, style = Stroke(width = 2f))
                }
            }
        }
    }
}

/**
 * Normalised (0..1) remote-screen coordinate for a touch position, accounting for
 * letterboxing, zoom and pan. Values outside 0..1 mean the touch is off the image.
 */
private fun calculateScreenRatio(
    touchPos: Offset,
    canvasSize: IntSize,
    bitmap: Bitmap?,
    scale: Float,
    offset: Offset
): Pair<Float, Float> {
    if (bitmap == null || canvasSize.width == 0 || canvasSize.height == 0) return -1f to -1f

    val viewWidth = canvasSize.width.toFloat()
    val viewHeight = canvasSize.height.toFloat()
    val imgWidth = bitmap.width.toFloat()
    val imgHeight = bitmap.height.toFloat()

    val scaleFactor = minOf(viewWidth / imgWidth, viewHeight / imgHeight)
    val baseDstWidth = imgWidth * scaleFactor
    val baseDstHeight = imgHeight * scaleFactor

    val baseOffsetX = (viewWidth - baseDstWidth) / 2f
    val baseOffsetY = (viewHeight - baseDstHeight) / 2f

    val finalWidth = baseDstWidth * scale
    val finalHeight = baseDstHeight * scale
    val finalX = baseOffsetX + offset.x - (finalWidth - baseDstWidth) / 2f
    val finalY = baseOffsetY + offset.y - (finalHeight - baseDstHeight) / 2f

    return ((touchPos.x - finalX) / finalWidth) to ((touchPos.y - finalY) / finalHeight)
}

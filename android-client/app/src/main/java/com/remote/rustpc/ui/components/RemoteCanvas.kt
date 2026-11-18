package com.remote.rustpc.ui.components

import android.graphics.Bitmap
import androidx.compose.foundation.Canvas
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
import com.remote.rustpc.data.MouseBtn
import com.remote.rustpc.network.RemoteConnection
import kotlin.math.abs
import kotlin.math.hypot
import kotlin.math.roundToInt

private const val LONG_PRESS_MS = 550L
private const val DOUBLE_TAP_MS = 350L
private const val DOUBLE_TAP_SLOP_PX = 60f
private const val SCROLL_STEP_PX = 28f

/**
 * Remote screen with touch input.
 *
 * Direct touch:  tap = click, double tap = double click, long press = right click,
 *                drag = left-button drag, two fingers = pinch zoom / pan.
 * Trackpad:      drag moves the cursor, tap = click, long press = right click,
 *                two fingers = scroll (or zoom when pinching).
 */
@Composable
fun RemoteCanvas(
    bitmap: Bitmap?,
    connection: RemoteConnection,
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

                awaitPointerEventScope {
                    var downTime = 0L
                    var downPos = Offset.Zero
                    var dragging = false     // left button currently held on the host
                    var multi = false        // a multi-finger gesture is in progress
                    var lastTapTime = 0L
                    var lastTapPos = Offset.Zero
                    var scrollAccum = 0f

                    fun ratioOf(p: Offset) =
                        calculateScreenRatio(p, canvasSize, bitmapState, scaleState, offsetState)

                    fun inside(r: Pair<Float, Float>) = r.first in 0f..1f && r.second in 0f..1f

                    while (true) {
                        val event = awaitPointerEvent()
                        val changes = event.changes
                        if (changes.isEmpty()) continue
                        val pressedCount = changes.count { it.pressed }

                        // ── Two or more fingers: zoom / pan / scroll ──
                        if (pressedCount >= 2) {
                            if (!multi) {
                                multi = true
                                if (dragging) {
                                    connection.sendMouseUp(MouseBtn.LEFT)
                                    dragging = false
                                }
                                scrollAccum = 0f
                            }
                            val zoom = event.calculateZoom()
                            val pan = event.calculatePan()
                            val pinching = abs(zoom - 1f) > 0.015f

                            if (!directState && scaleState <= 1.01f && !pinching) {
                                // Trackpad: two-finger drag scrolls the remote page.
                                scrollAccum += pan.y
                                while (abs(scrollAccum) >= SCROLL_STEP_PX) {
                                    val dir = if (scrollAccum > 0) 1 else -1
                                    connection.sendMouseScroll(0, 120 * dir)
                                    scrollAccum -= SCROLL_STEP_PX * dir
                                }
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
                                val canClick = if (directState) {
                                    val r = ratioOf(downPos)
                                    if (inside(r)) connection.sendMouseMove(r.first, r.second)
                                    inside(r)
                                } else true

                                if (canClick) {
                                    if (elapsed >= LONG_PRESS_MS) {
                                        connection.sendMouseClick(MouseBtn.RIGHT)
                                        lastTapTime = 0L
                                    } else {
                                        // Each tap is a plain click. Two quick taps in the same
                                        // place reach the host as two clicks, which Windows treats
                                        // as a double-click.
                                        connection.sendMouseClick(MouseBtn.LEFT)
                                        val isSecond = now - lastTapTime < DOUBLE_TAP_MS &&
                                            hypot(
                                                (downPos.x - lastTapPos.x).toDouble(),
                                                (downPos.y - lastTapPos.y).toDouble()
                                            ) < DOUBLE_TAP_SLOP_PX
                                        lastTapTime = if (isSecond) 0L else now
                                        lastTapPos = downPos
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

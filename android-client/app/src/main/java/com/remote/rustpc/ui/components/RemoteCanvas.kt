package com.remote.rustpc.ui.components

import android.graphics.Bitmap
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.calculateCentroid
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
import androidx.compose.ui.input.pointer.PointerInputChange
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.positionChange
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import com.remote.rustpc.data.MouseBtn
import com.remote.rustpc.network.RemoteConnection
import kotlin.math.hypot
import kotlin.math.roundToInt

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

    Box(
        modifier = modifier
            .fillMaxSize()
            .pointerInput(isDirectTouch, bitmap, canvasSize, scale, offset) {
                awaitPointerEventScope {
                    var downTime = 0L
                    var downPos = Offset.Zero
                    var isDragging = false
                    var lastTapTime = 0L

                    while (true) {
                        val event = awaitPointerEvent()
                        val changes = event.changes

                        if (changes.isEmpty()) continue

                        if (changes.size >= 2) {
                            // 2-finger gesture: Zoom & Pan
                            val zoom = event.calculateZoom()
                            val pan = event.calculatePan()
                            if (zoom != 1f || pan != Offset.Zero) {
                                onTransform(zoom, pan)
                                changes.forEach { it.consume() }
                            }
                            continue
                        }

                        // Single finger gesture
                        val change = changes[0]

                        if (change.pressed && !change.previousPressed) {
                            // Finger Down
                            downTime = System.currentTimeMillis()
                            downPos = change.position
                            isDragging = false

                            if (isDirectTouch) {
                                val (rx, ry) = calculateScreenRatio(downPos, canvasSize, bitmap, scale, offset)
                                if (rx in 0f..1f && ry in 0f..1f) {
                                    connection.sendMouseMove(rx, ry)
                                }
                            }
                            change.consume()
                        } else if (change.pressed && change.previousPressed) {
                            // Finger Moved
                            val dist = hypot(
                                (change.position.x - downPos.x).toDouble(),
                                (change.position.y - downPos.y).toDouble()
                            ).toFloat()

                            if (dist > 18f) {
                                isDragging = true
                            }

                            if (isDirectTouch) {
                                val (rx, ry) = calculateScreenRatio(change.position, canvasSize, bitmap, scale, offset)
                                if (rx in 0f..1f && ry in 0f..1f) {
                                    if (isDragging) {
                                        // Send mouse down if starting drag
                                        connection.sendMouseDown(MouseBtn.LEFT)
                                    }
                                    connection.sendMouseMove(rx, ry)
                                }
                            } else {
                                // Trackpad mode: move cursor relatively
                                val pan = change.positionChange()
                                val sensitivity = 0.0018f / scale.coerceAtLeast(1f)
                                cursorRatioX = (cursorRatioX + pan.x * sensitivity).coerceIn(0f, 1f)
                                cursorRatioY = (cursorRatioY + pan.y * sensitivity).coerceIn(0f, 1f)
                                connection.sendMouseMove(cursorRatioX, cursorRatioY)
                            }
                            change.consume()
                        } else if (!change.pressed && change.previousPressed) {
                            // Finger Released (Up)
                            val elapsed = System.currentTimeMillis() - downTime

                            if (isDirectTouch) {
                                if (isDragging) {
                                    connection.sendMouseUp(MouseBtn.LEFT)
                                } else {
                                    // Tap or Long press
                                    val (rx, ry) = calculateScreenRatio(downPos, canvasSize, bitmap, scale, offset)
                                    if (rx in 0f..1f && ry in 0f..1f) {
                                        connection.sendMouseMove(rx, ry)

                                        if (elapsed >= 550) {
                                            // Long press -> Right Click
                                            connection.sendMouseClick(MouseBtn.RIGHT)
                                        } else {
                                            val now = System.currentTimeMillis()
                                            if (now - lastTapTime < 350) {
                                                // Double tap
                                                connection.sendMouseDoubleClick(MouseBtn.LEFT)
                                                lastTapTime = 0L
                                            } else {
                                                // Single click
                                                connection.sendMouseClick(MouseBtn.LEFT)
                                                lastTapTime = now
                                            }
                                        }
                                    }
                                }
                            } else {
                                // Trackpad Mode Release
                                if (!isDragging) {
                                    if (elapsed >= 550) {
                                        connection.sendMouseClick(MouseBtn.RIGHT)
                                    } else {
                                        val now = System.currentTimeMillis()
                                        if (now - lastTapTime < 350) {
                                            connection.sendMouseDoubleClick(MouseBtn.LEFT)
                                            lastTapTime = 0L
                                        } else {
                                            connection.sendMouseClick(MouseBtn.LEFT)
                                            lastTapTime = now
                                        }
                                    }
                                }
                            }
                            change.consume()
                        }
                    }
                }
            }
    ) {
        Canvas(modifier = Modifier.fillMaxSize()) {
            canvasSize = size.let { IntSize(it.width.roundToInt(), it.height.roundToInt()) }

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

                // Draw remote screen
                drawImage(
                    image = imageBitmap,
                    dstOffset = IntOffset(finalX.roundToInt(), finalY.roundToInt()),
                    dstSize = IntSize(finalWidth.roundToInt(), finalHeight.roundToInt())
                )

                // If in Trackpad mode, draw a sleek cursor pointer
                if (!isDirectTouch) {
                    val cursorScreenX = finalX + cursorRatioX * finalWidth
                    val cursorScreenY = finalY + cursorRatioY * finalHeight

                    val cursorPath = Path().apply {
                        moveTo(cursorScreenX, cursorScreenY)
                        lineTo(cursorScreenX + 18f, cursorScreenY + 14f)
                        lineTo(cursorScreenX + 8f, cursorScreenY + 16f)
                        lineTo(cursorScreenX + 5f, cursorScreenY + 24f)
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
 * Calculates normalized (0.0 .. 1.0) remote screen coordinate for a touch position
 */
private fun calculateScreenRatio(
    touchPos: Offset,
    canvasSize: IntSize,
    bitmap: Bitmap?,
    scale: Float,
    offset: Offset
): Pair<Float, Float> {
    if (bitmap == null || canvasSize.width == 0 || canvasSize.height == 0) return 0f to 0f

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

    val xRatio = (touchPos.x - finalX) / finalWidth
    val yRatio = (touchPos.y - finalY) / finalHeight

    return xRatio to yRatio
}

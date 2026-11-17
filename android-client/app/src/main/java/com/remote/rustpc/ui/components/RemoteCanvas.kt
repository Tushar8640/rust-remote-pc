package com.remote.rustpc.ui.components

import android.graphics.Bitmap
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.rememberTransformableState
import androidx.compose.foundation.gestures.transformable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import com.remote.rustpc.data.MouseBtn
import com.remote.rustpc.network.RemoteConnection
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

    // Trackpad virtual cursor normalized position (starts center)
    var virtualCursorX by remember { mutableFloatStateOf(0.5f) }
    var virtualCursorY by remember { mutableFloatStateOf(0.5f) }

    val transformState = rememberTransformableState { zoomChange, panChange, _ ->
        onTransform(zoomChange, panChange)
    }

    Box(
        modifier = modifier
            .fillMaxSize()
            .transformable(state = transformState)
            .pointerInput(isDirectTouch, bitmap, canvasSize, scale, offset) {
                if (isDirectTouch) {
                    // Direct touch: tap & drag maps directly to screen coordinates
                    detectTapGestures(
                        onTap = { touchPos ->
                            val (rx, ry) = calculateScreenRatio(touchPos, canvasSize, bitmap, scale, offset)
                            if (rx in 0f..1f && ry in 0f..1f) {
                                connection.sendMouseMove(rx, ry)
                                connection.sendMouseClick(MouseBtn.LEFT)
                            }
                        },
                        onDoubleTap = { touchPos ->
                            val (rx, ry) = calculateScreenRatio(touchPos, canvasSize, bitmap, scale, offset)
                            if (rx in 0f..1f && ry in 0f..1f) {
                                connection.sendMouseMove(rx, ry)
                                connection.sendMouseClick(MouseBtn.LEFT)
                                connection.sendMouseClick(MouseBtn.LEFT)
                            }
                        },
                        onLongPress = { touchPos ->
                            val (rx, ry) = calculateScreenRatio(touchPos, canvasSize, bitmap, scale, offset)
                            if (rx in 0f..1f && ry in 0f..1f) {
                                connection.sendMouseMove(rx, ry)
                                connection.sendMouseClick(MouseBtn.RIGHT)
                            }
                        }
                    )
                } else {
                    // Trackpad mode: tap to click, two-finger right click, drag moves cursor
                    detectTapGestures(
                        onTap = {
                            connection.sendMouseClick(MouseBtn.LEFT)
                        },
                        onDoubleTap = {
                            connection.sendMouseClick(MouseBtn.LEFT)
                            connection.sendMouseClick(MouseBtn.LEFT)
                        },
                        onLongPress = {
                            connection.sendMouseClick(MouseBtn.RIGHT)
                        }
                    )
                }
            }
            .pointerInput(isDirectTouch, bitmap, canvasSize, scale, offset) {
                if (isDirectTouch) {
                    detectDragGestures(
                        onDragStart = { startPos ->
                            val (rx, ry) = calculateScreenRatio(startPos, canvasSize, bitmap, scale, offset)
                            if (rx in 0f..1f && ry in 0f..1f) {
                                connection.sendMouseMove(rx, ry)
                                connection.sendMouseDown(MouseBtn.LEFT)
                            }
                        },
                        onDragEnd = {
                            connection.sendMouseUp(MouseBtn.LEFT)
                        },
                        onDragCancel = {
                            connection.sendMouseUp(MouseBtn.LEFT)
                        },
                        onDrag = { change, _ ->
                            change.consume()
                            val (rx, ry) = calculateScreenRatio(change.position, canvasSize, bitmap, scale, offset)
                            if (rx in 0f..1f && ry in 0f..1f) {
                                connection.sendMouseMove(rx, ry)
                            }
                        }
                    )
                } else {
                    // Virtual Trackpad relative drag
                    detectDragGestures(
                        onDrag = { change, dragAmount ->
                            change.consume()
                            val sensitivity = 0.0012f / scale.coerceAtLeast(1f)
                            virtualCursorX = (virtualCursorX + dragAmount.x * sensitivity).coerceIn(0f, 1f)
                            virtualCursorY = (virtualCursorY + dragAmount.y * sensitivity).coerceIn(0f, 1f)
                            connection.sendMouseMove(virtualCursorX, virtualCursorY)
                        }
                    )
                }
            }
    ) {
        Canvas(modifier = Modifier.fillMaxSize()) {
            canvasSize = size.let { IntSize(it.width.roundToInt(), it.height.roundToInt()) }

            if (bitmap != null && !bitmap.isRecycled) {
                val imageBitmap = bitmap.asImageBitmap()

                // Calculate aspect ratio preserving fit
                val viewWidth = size.width
                val viewHeight = size.height
                val imgWidth = bitmap.width.toFloat()
                val imgHeight = bitmap.height.toFloat()

                val scaleFactor = minOf(viewWidth / imgWidth, viewHeight / imgHeight)
                val baseDstWidth = imgWidth * scaleFactor
                val baseDstHeight = imgHeight * scaleFactor

                val baseOffsetX = (viewWidth - baseDstWidth) / 2f
                val baseOffsetY = (viewHeight - baseDstHeight) / 2f

                // Apply user pinch zoom and pan
                val finalWidth = baseDstWidth * scale
                val finalHeight = baseDstHeight * scale
                val finalX = baseOffsetX + offset.x - (finalWidth - baseDstWidth) / 2f
                val finalY = baseOffsetY + offset.y - (finalHeight - baseDstHeight) / 2f

                drawImage(
                    image = imageBitmap,
                    dstOffset = IntOffset(finalX.roundToInt(), finalY.roundToInt()),
                    dstSize = IntSize(finalWidth.roundToInt(), finalHeight.roundToInt())
                )
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

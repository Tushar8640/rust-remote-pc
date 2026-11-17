package com.remote.rustpc.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.remote.rustpc.network.ConnectionStats
import com.remote.rustpc.ui.theme.*

@Composable
fun StatBadge(
    stats: ConnectionStats,
    modifier: Modifier = Modifier
) {
    val rttColor = when {
        stats.rttMs == 0L -> AccentCyan
        stats.rttMs < 30 -> AccentGreen
        stats.rttMs < 80 -> Color(0xFFFBBF24) // Yellow
        else -> AccentRed
    }

    Row(
        modifier = modifier
            .clip(RoundedCornerShape(20.dp))
            .background(SurfaceDark.copy(alpha = 0.85f))
            .padding(horizontal = 12.dp, vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp)
    ) {
        // Status Dot
        Box(
            modifier = Modifier
                .size(8.dp)
                .clip(RoundedCornerShape(4.dp))
                .background(AccentGreen)
        )

        // FPS
        Text(
            text = "${stats.fps} FPS",
            style = Typography.labelSmall.copy(fontSize = 12.sp, color = AccentCyan)
        )

        Text(
            text = "•",
            style = Typography.labelSmall.copy(color = TextSecondary)
        )

        // Latency RTT
        Text(
            text = if (stats.rttMs > 0) "${stats.rttMs} ms" else "< 1 ms",
            style = Typography.labelSmall.copy(fontSize = 12.sp, color = rttColor)
        )

        if (stats.screenWidth > 0 && stats.screenHeight > 0) {
            Text(
                text = "•",
                style = Typography.labelSmall.copy(color = TextSecondary)
            )
            // Resolution
            Text(
                text = "${stats.screenWidth}x${stats.screenHeight}",
                style = Typography.labelSmall.copy(fontSize = 11.sp, color = TextSecondary)
            )
        }
    }
}

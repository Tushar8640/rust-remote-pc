package com.remote.rustpc.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.VolumeOff
import androidx.compose.material.icons.automirrored.filled.VolumeUp
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.remote.rustpc.network.ConnectionStats
import com.remote.rustpc.ui.theme.*

@Composable
fun StatBadge(
    stats: ConnectionStats,
    audioActive: Boolean,
    audioMuted: Boolean,
    modifier: Modifier = Modifier
) {
    val (quality, qualityColor) = when {
        stats.rttMs == 0L -> "Live" to AccentIndigoLight
        stats.rttMs < 30 -> "Excellent" to AccentGreen
        stats.rttMs < 80 -> "Good" to AccentAmber
        else -> "Slow" to AccentRed
    }

    Row(
        modifier = modifier
            .clip(RoundedCornerShape(20.dp))
            .background(BgDark.copy(alpha = 0.82f))
            .border(1.dp, BorderDark, RoundedCornerShape(20.dp))
            .padding(horizontal = 12.dp, vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp)
    ) {
        Box(
            modifier = Modifier
                .size(8.dp)
                .clip(CircleShape)
                .background(qualityColor)
        )
        Text(quality, style = Typography.labelMedium.copy(color = TextPrimary))
        Text(
            text = "${stats.rttMs} ms · ${stats.fps} fps",
            style = Typography.labelSmall.copy(fontSize = 11.sp, color = TextSecondary)
        )
        if (stats.screenWidth > 0 && stats.screenHeight > 0) {
            Text(
                text = "${stats.screenWidth}×${stats.screenHeight}",
                style = Typography.labelSmall.copy(fontSize = 11.sp, color = TextFaint)
            )
        }
        if (audioActive) {
            Icon(
                imageVector = if (audioMuted) Icons.AutoMirrored.Filled.VolumeOff else Icons.AutoMirrored.Filled.VolumeUp,
                contentDescription = if (audioMuted) "Audio muted" else "Audio on",
                tint = if (audioMuted) TextFaint else AccentGreen,
                modifier = Modifier.size(14.dp)
            )
        }
    }
}

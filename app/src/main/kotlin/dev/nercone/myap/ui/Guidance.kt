package dev.nercone.myap.ui

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.VolumeOff
import androidx.compose.material.icons.automirrored.filled.VolumeUp
import androidx.compose.material.icons.filled.Block
import androidx.compose.material.icons.filled.CameraAlt
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.ExpandLess
import androidx.compose.material.icons.filled.ExpandMore
import androidx.compose.material.icons.filled.GpsOff
import androidx.compose.material.icons.filled.Map
import androidx.compose.material.icons.filled.Merge
import androidx.compose.material.icons.filled.MyLocation
import androidx.compose.material.icons.filled.Straight
import androidx.compose.material.icons.filled.TurnLeft
import androidx.compose.material.icons.filled.TurnRight
import androidx.compose.material.icons.filled.TurnSharpLeft
import androidx.compose.material.icons.filled.TurnSharpRight
import androidx.compose.material.icons.filled.TurnSlightLeft
import androidx.compose.material.icons.filled.TurnSlightRight
import androidx.compose.material.icons.filled.UTurnLeft
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.FilledTonalIconButton
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.LinearWavyProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.SmallFloatingActionButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.nercone.myap.R
import dev.nercone.myap.core.DrivingSide
import dev.nercone.myap.core.Lane
import dev.nercone.myap.core.LaneIndication
import dev.nercone.myap.core.NavigationState
import dev.nercone.myap.core.NavigationView
import dev.nercone.myap.core.Snapshot
import dev.nercone.myap.core.StepView

/**
 * The screen of the guidance, over the map: the next maneuver large at the top (its distance,
 * what to do, the junction, the signs and the lanes), the one after it when it follows closely,
 * warnings of what is ahead, the speed limit and the speed, and at the bottom the time, distance
 * and arrival with the actions of the guidance: the overview of the route, the voice, the camera
 * view, avoiding the road ahead, and ending the guidance. Every text and decision is the core's.
 */
object Guidance {
    /** What the guidance screen can do besides ending. */
    class Actions(
        val recenter: () -> Unit,
        val stop: () -> Unit,
        val overview: () -> Unit,
        val camera: () -> Unit,
        val mute: () -> Unit,
        val avoidAhead: () -> Unit,
        val acceptFaster: () -> Unit,
        val dismissFaster: () -> Unit,
    )

    @OptIn(ExperimentalMaterial3ExpressiveApi::class)
    @Composable
    fun Content(snapshot: Snapshot, nav: NavigationView, following: Boolean, overview: Boolean, actions: Actions) {
        var expanded by rememberSaveable { mutableStateOf(false) }
        var confirmAvoid by rememberSaveable { mutableStateOf(false) }
        // The buttons at the sides stand above the panel at the bottom, whatever its height (the
        // steps open, a short screen in landscape).
        val density = LocalDensity.current
        var panel by remember { mutableStateOf(0.dp) }
        val screen = LocalConfiguration.current.screenHeightDp.dp
        Box(Modifier.fillMaxSize()) {
            Column(Modifier.align(Alignment.TopCenter).statusBarsPadding().padding(12.dp).widthIn(max = 720.dp).fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                Banner(nav)
                nav.thenIcon?.let { icon ->
                    Surface(color = MapColors.GUIDANCE.copy(alpha = 0.85f), contentColor = MapColors.ON_GUIDANCE, shape = RoundedCornerShape(16.dp), modifier = Modifier.padding(start = 16.dp)) {
                        Row(Modifier.padding(horizontal = 12.dp, vertical = 6.dp), verticalAlignment = Alignment.CenterVertically) {
                            Text(stringResource(R.string.then), style = MaterialTheme.typography.labelLarge)
                            Spacer(Modifier.width(6.dp))
                            Icon(Labels.maneuver(icon, nav.drivingSide), null, Modifier.size(22.dp))
                        }
                    }
                }
                if (nav.state == NavigationState.REROUTING) Chip(stringResource(R.string.rerouting), MaterialTheme.colorScheme.tertiaryContainer, MaterialTheme.colorScheme.onTertiaryContainer)
                if (nav.estimated) Chip(stringResource(R.string.estimated_position), MaterialTheme.colorScheme.surfaceContainerHigh, MaterialTheme.colorScheme.onSurface, Icons.Default.GpsOff)
                nav.alerts.forEach { alert -> Chip("${alert.name} · ${alert.distance}", Color(0xFFFFE08A), Color(0xFF3D2E00), Labels.alert(alert.icon)) }
                nav.congestion?.let { Chip(it, Color(0xFFF6C1BC), Color(0xFF5C0F0A), Icons.Default.Warning) }
                nav.fasterRoute?.let { faster ->
                    Surface(shape = RoundedCornerShape(20.dp), color = MaterialTheme.colorScheme.primaryContainer, shadowElevation = 4.dp, modifier = Modifier.fillMaxWidth().semantics { liveRegion = LiveRegionMode.Polite }) {
                        Column(Modifier.padding(16.dp)) {
                            Text(faster.text, style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.SemiBold)
                            if (faster.summary.isNotEmpty()) Text(faster.summary, style = MaterialTheme.typography.bodyMedium)
                            Row(Modifier.padding(top = 8.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                Button(onClick = actions.acceptFaster) { Text(stringResource(R.string.faster_route_take)) }
                                OutlinedButton(onClick = actions.dismissFaster) { Text(stringResource(R.string.faster_route_keep)) }
                            }
                        }
                    }
                }
            }
            Column(Modifier.align(Alignment.BottomStart).padding(start = 16.dp, bottom = panel + 16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                nav.speedLimit?.let { SpeedLimit(it) }
                nav.speed?.let { speed ->
                    Surface(shape = RoundedCornerShape(12.dp), color = if (nav.speeding) Color(0xFFD93025) else MaterialTheme.colorScheme.surface, contentColor = if (nav.speeding) Color.White else MaterialTheme.colorScheme.onSurface, shadowElevation = 3.dp) {
                        Column(Modifier.padding(horizontal = 10.dp, vertical = 4.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                            Text("$speed", style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.Bold)
                            Text(if (snapshot.imperial) "mph" else "km/h", style = MaterialTheme.typography.labelSmall)
                        }
                    }
                }
            }
            Column(Modifier.align(Alignment.BottomEnd).padding(end = 16.dp, bottom = panel + 16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                if (!following || overview) {
                    SmallFloatingActionButton(onClick = actions.recenter) { Icon(Icons.Default.MyLocation, stringResource(R.string.recenter)) }
                }
                SmallFloatingActionButton(onClick = actions.overview) { Icon(Icons.Default.Map, stringResource(R.string.route_overview)) }
                SmallFloatingActionButton(onClick = actions.mute) {
                    if (snapshot.settings.voice) Icon(Icons.AutoMirrored.Filled.VolumeUp, stringResource(R.string.voice_mute)) else Icon(Icons.AutoMirrored.Filled.VolumeOff, stringResource(R.string.voice_unmute))
                }
                SmallFloatingActionButton(onClick = actions.camera) { Icon(Icons.Default.CameraAlt, stringResource(R.string.ar_title)) }
            }
            Surface(
                shape = RoundedCornerShape(topStart = 28.dp, topEnd = 28.dp),
                shadowElevation = 8.dp,
                modifier = Modifier.align(Alignment.BottomCenter).fillMaxWidth().onSizeChanged { panel = with(density) { it.height.toDp() } },
            ) {
                Column(Modifier.navigationBarsPadding().padding(horizontal = 20.dp, vertical = 12.dp)) {
                    LinearWavyProgressIndicator(progress = { nav.progress.toFloat() }, modifier = Modifier.fillMaxWidth())
                    Row(Modifier.padding(top = 12.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                        Column(Modifier.weight(1f).clickable { expanded = !expanded }) {
                            Row(verticalAlignment = Alignment.CenterVertically) {
                                Text(nav.remainingTime, style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.SemiBold, color = if ((nav.delayMinutes ?: 0) >= 5) Color(0xFFD93025) else MaterialTheme.colorScheme.primary)
                                Icon(if (expanded) Icons.Default.ExpandMore else Icons.Default.ExpandLess, stringResource(R.string.upcoming_steps), Modifier.padding(start = 4.dp))
                            }
                            Text("${nav.remainingDistance} · ${stringResource(R.string.arrive_at, nav.arrival)} · ${Labels.router(nav.router)}", style = MaterialTheme.typography.bodyMedium, color = Widgets.secondary)
                            nav.delayMinutes?.takeIf { it > 0 }?.let { Text(stringResource(R.string.traffic_delay, it), style = MaterialTheme.typography.bodySmall, color = Color(0xFFD93025)) }
                        }
                        FilledTonalIconButton(onClick = { confirmAvoid = true }, modifier = Modifier.size(48.dp)) { Icon(Icons.Default.Block, stringResource(R.string.avoid_ahead)) }
                        FilledTonalIconButton(onClick = actions.stop, modifier = Modifier.size(56.dp)) { Icon(Icons.Default.Close, stringResource(R.string.end_navigation)) }
                    }
                    AnimatedVisibility(expanded) {
                        Column(Modifier.padding(top = 8.dp).heightIn(max = screen * 0.4f).verticalScroll(rememberScrollState())) {
                            HorizontalDivider()
                            if (nav.upcoming.isEmpty()) Text(stringResource(R.string.no_more_steps), Modifier.padding(vertical = 12.dp), color = Widgets.secondary)
                            nav.upcoming.forEach { StepRow(it, nav.drivingSide) }
                        }
                    }
                    Text(snapshot.style.attribution, style = MaterialTheme.typography.labelSmall, color = Widgets.secondary)
                }
            }
        }
        if (confirmAvoid) {
            Widgets.Confirm(
                title = stringResource(R.string.avoid_ahead),
                text = stringResource(R.string.avoid_ahead_text),
                confirm = stringResource(R.string.avoid_ahead_confirm),
                onConfirm = {
                    confirmAvoid = false
                    actions.avoidAhead()
                },
                onDismiss = { confirmAvoid = false },
            )
        }
    }

    /** The next maneuver: its icon, distance and instruction, the junction, the signs and the lanes. */
    @Composable
    fun Banner(nav: NavigationView) {
        Surface(
            color = MapColors.GUIDANCE,
            contentColor = MapColors.ON_GUIDANCE,
            shape = RoundedCornerShape(24.dp),
            shadowElevation = 6.dp,
            modifier = Modifier.fillMaxWidth(),
        ) {
            Column(Modifier.padding(16.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Icon(Labels.maneuver(nav.icon, nav.drivingSide), null, Modifier.size(56.dp))
                    Spacer(Modifier.width(16.dp))
                    Column(Modifier.weight(1f)) {
                        if (nav.state != NavigationState.ARRIVED) Text(nav.distance, style = MaterialTheme.typography.headlineMedium, fontWeight = FontWeight.Bold)
                        // Only the instruction is a live region: the distance changes every second
                        // and would be read out again and again.
                        Text(nav.instruction, style = MaterialTheme.typography.titleMedium, maxLines = 3, overflow = TextOverflow.Ellipsis, modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite })
                    }
                }
                val sign = listOfNotNull(nav.exitNumber, nav.destinations).joinToString("  ")
                if (sign.isNotEmpty() && nav.state != NavigationState.ARRIVED) {
                    // As the signs of roads: white on blue (green on motorways is not known here).
                    Surface(color = Color(0xFF1D4FA3), contentColor = Color.White, shape = RoundedCornerShape(8.dp), border = BorderStroke(2.dp, Color.White), modifier = Modifier.padding(top = 10.dp)) {
                        Text(sign, Modifier.padding(horizontal = 10.dp, vertical = 4.dp), style = MaterialTheme.typography.titleSmall, fontWeight = FontWeight.SemiBold, maxLines = 2, overflow = TextOverflow.Ellipsis)
                    }
                }
                if (nav.lanes.isNotEmpty() && nav.state != NavigationState.ARRIVED && nav.distanceM < 800) Lanes(nav.lanes, Modifier.padding(top = 10.dp))
            }
        }
    }

    /** The lanes before the maneuver, the ones to take bright. */
    @Composable
    fun Lanes(lanes: List<Lane>, modifier: Modifier = Modifier) {
        val described = lanes.joinToString { lane -> (if (lane.valid) "✓ " else "") + lane.indications.joinToString("/") { it.name.lowercase() } }
        Row(modifier.background(Color.Black.copy(alpha = 0.25f), RoundedCornerShape(10.dp)).padding(horizontal = 6.dp, vertical = 4.dp).semantics { contentDescription = described }, horizontalArrangement = Arrangement.spacedBy(2.dp)) {
            lanes.forEach { lane ->
                Box(Modifier.size(width = 34.dp, height = 38.dp).border(if (lane.preferred) 2.dp else 0.dp, Color.White, RoundedCornerShape(6.dp)), contentAlignment = Alignment.Center) {
                    val tint = if (lane.valid) Color.White else Color.White.copy(alpha = 0.35f)
                    Box(contentAlignment = Alignment.Center) {
                        lane.indications.forEach { indication -> laneIcon(indication)?.let { Icon(it, null, Modifier.size(28.dp), tint = tint) } }
                        if (lane.indications.all { it == LaneIndication.NONE }) Icon(Icons.Default.Straight, null, Modifier.size(28.dp), tint = tint)
                    }
                }
            }
        }
    }

    private fun laneIcon(indication: LaneIndication): ImageVector? = when (indication) {
        LaneIndication.SHARP_LEFT -> Icons.Default.TurnSharpLeft
        LaneIndication.LEFT -> Icons.Default.TurnLeft
        LaneIndication.SLIGHT_LEFT -> Icons.Default.TurnSlightLeft
        LaneIndication.THROUGH -> Icons.Default.Straight
        LaneIndication.SLIGHT_RIGHT -> Icons.Default.TurnSlightRight
        LaneIndication.RIGHT -> Icons.Default.TurnRight
        LaneIndication.SHARP_RIGHT -> Icons.Default.TurnSharpRight
        LaneIndication.U_TURN -> Icons.Default.UTurnLeft
        LaneIndication.MERGE_TO_LEFT, LaneIndication.MERGE_TO_RIGHT -> Icons.Default.Merge
        LaneIndication.NONE -> null
    }

    /** A speed limit sign: the number in a red ring. */
    @Composable
    fun SpeedLimit(limit: Int) {
        Box(
            Modifier.size(56.dp).background(Color.White, CircleShape).border(6.dp, Color(0xFFD93025), CircleShape).semantics { contentDescription = "$limit" },
            contentAlignment = Alignment.Center,
        ) {
            Text("$limit", color = Color.Black, fontWeight = FontWeight.Bold, fontSize = if (limit >= 100) 18.sp else 22.sp)
        }
    }

    @Composable
    private fun Chip(text: String, color: Color, content: Color, icon: ImageVector? = null) {
        Surface(color = color, contentColor = content, shape = RoundedCornerShape(16.dp), shadowElevation = 2.dp) {
            Row(Modifier.padding(horizontal = 14.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                icon?.let {
                    Icon(it, null, Modifier.size(20.dp))
                    Spacer(Modifier.width(8.dp))
                }
                Text(text, style = MaterialTheme.typography.labelLarge, maxLines = 2, overflow = TextOverflow.Ellipsis)
            }
        }
    }

    @Composable
    private fun StepRow(step: StepView, side: DrivingSide) {
        Row(Modifier.fillMaxWidth().padding(vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            Icon(Labels.maneuver(step.icon, side), null, Modifier.size(28.dp))
            Spacer(Modifier.width(12.dp))
            Column(Modifier.weight(1f)) {
                Text(step.instruction, style = MaterialTheme.typography.bodyLarge, maxLines = 2, overflow = TextOverflow.Ellipsis)
                if (step.lanes.isNotEmpty()) Text(step.lanes.joinToString(" ") { if (it.valid) "▲" else "△" }, style = MaterialTheme.typography.labelSmall, color = Widgets.secondary)
            }
            Text(step.distance, style = MaterialTheme.typography.labelLarge, color = Widgets.secondary)
        }
    }
}

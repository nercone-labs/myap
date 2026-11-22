package dev.nercone.myap.ui

import android.os.Build
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.MaterialExpressiveTheme
import androidx.compose.material3.MotionScheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.dynamicLightColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext

/**
 * Material 3 Expressive with the colours of the wallpaper (Material You), and expressive motion.
 * Before Android 12, which has no colours of the wallpaper, a scheme of the icon's green. The
 * colours of the map's own marks (routes, places) are fixed (`MapColors`): they must stand out on
 * every map the same way.
 */
object MyapTheme {
    private val lightFallback = lightColorScheme(primary = Color(0xFF006C47), onPrimary = Color.White, primaryContainer = Color(0xFF8EF7C1), onPrimaryContainer = Color(0xFF002113))
    private val darkFallback = darkColorScheme(primary = Color(0xFF71DAA6), onPrimary = Color(0xFF003822), primaryContainer = Color(0xFF005234), onPrimaryContainer = Color(0xFF8EF7C1))

    @OptIn(ExperimentalMaterial3ExpressiveApi::class)
    @Composable
    fun Content(content: @Composable () -> Unit) {
        val context = LocalContext.current
        val dark = isSystemInDarkTheme()
        val colors = when {
            Build.VERSION.SDK_INT >= Build.VERSION_CODES.S -> if (dark) dynamicDarkColorScheme(context) else dynamicLightColorScheme(context)
            dark -> darkFallback
            else -> lightFallback
        }
        MaterialExpressiveTheme(colorScheme = colors, motionScheme = MotionScheme.expressive(), content = content)
    }
}

/** The fixed colours of what Myap draws on the map, and of the guidance banner. */
object MapColors {
    const val ROUTE = "#1A73E8"
    const val ROUTE_CASING = "#0B57D0"
    const val ROUTE_ALTERNATIVE = "#8EB5F5"
    const val ROUTE_PASSED = "#9AA0A6"
    const val RESULT = "#D93025"
    const val ORIGIN = "#FFFFFF"
    const val STOP = "#F29900"
    const val SAVED = "#F9AB00"
    const val ME = "#1A73E8"
    const val AVOID = "#D93025"
    const val ALERT = "#FBBC04"
    val GUIDANCE = Color(0xFF0B6E4F)
    val ON_GUIDANCE = Color.White
}

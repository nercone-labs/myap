import org.gradle.internal.os.OperatingSystem

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
    alias(libs.plugins.kotlin.serialization)
}

/**
 * Builds the Rust core (the library `myap_ffi` of `crates/myap-ffi`) for one Android
 * ABI with Cargo, linking with the LLVM toolchain of the NDK, into the directory of the native
 * libraries of that ABI.
 */
abstract class CargoBuild : DefaultTask() {
    @get:Input abstract val cargo: Property<String>
    @get:Input abstract val abi: Property<String>
    @get:Input abstract val target: Property<String>
    @get:Input abstract val linker: Property<String>
    @get:Internal abstract val workspace: DirectoryProperty
    @get:InputFiles abstract val sources: ConfigurableFileCollection
    @get:OutputDirectory abstract val outputDirectory: DirectoryProperty

    @get:Inject abstract val execOperations: ExecOperations

    @TaskAction
    fun build() {
        val triple = target.get()
        val variable = "CARGO_TARGET_" + triple.uppercase().replace('-', '_') + "_LINKER"
        execOperations.exec {
            workingDir = workspace.get().asFile
            environment(variable, linker.get())
            commandLine(cargo.get(), "build", "--release", "--locked", "-p", "myap-ffi", "--target", triple)
        }
        val library = workspace.get().file("target/$triple/release/libmyap_ffi.so").asFile
        // Emptied first, so that a library of another name (an earlier one) is never packaged too.
        val directory = outputDirectory.get().dir(abi.get()).asFile
        directory.deleteRecursively()
        directory.mkdirs()
        library.copyTo(directory.resolve("libmyap_ffi.so"), overwrite = true)
    }
}

val allAbis = mapOf(
    "arm64-v8a" to "aarch64-linux-android",
    "armeabi-v7a" to "armv7-linux-androideabi",
    "x86_64" to "x86_64-linux-android",
)
// Every ABI is built unless `-Pengine.abis=arm64-v8a,x86_64` names some, for a local build on a
// machine without every Rust target. A name that is not an ABI above is an error, not ignored.
val abis = providers.gradleProperty("engine.abis").orNull?.split(',')?.map { it.trim() }?.filter { it.isNotEmpty() }?.let { names ->
    names.forEach { require(it in allAbis) { "engine.abis: unknown ABI $it; known: ${allAbis.keys.joinToString()}" } }
    allAbis.filterKeys { it in names }
} ?: allAbis
android {
    namespace = "dev.nercone.myap"
    compileSdk = 37
    ndkVersion = "30.0.16248370"

    defaultConfig {
        applicationId = "dev.nercone.myap"
        minSdk = 26
        targetSdk = 37
        versionCode = 100
        versionName = "0.1.0"
        // Only the ABIs the core is built for: MapLibre ships its library for more (x86 among
        // them), and a device of such an ABI would find MapLibre but not the core, and fail at
        // start.
        ndk { abiFilters += abis.keys }
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
        }
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }
}

val clangPrefix = mapOf(
    "aarch64-linux-android" to "aarch64-linux-android",
    "armv7-linux-androideabi" to "armv7a-linux-androideabi",
    "x86_64-linux-android" to "x86_64-linux-android",
)
val hostTag = when {
    OperatingSystem.current().isMacOsX -> "darwin-x86_64"
    OperatingSystem.current().isWindows -> "windows-x86_64"
    else -> "linux-x86_64"
}
val workspaceDirectory = rootProject.layout.projectDirectory
val minimumSdk = 26
// Cargo from the CARGO variable, the rustup default location, or the PATH, in that order.
val cargoExecutable = providers.environmentVariable("CARGO").orElse(
    providers.systemProperty("user.home").map { home ->
        val candidate = File(home, ".cargo/bin/cargo" + if (OperatingSystem.current().isWindows) ".exe" else "")
        if (candidate.canExecute()) candidate.absolutePath else "cargo"
    },
)

val cargoTasks = abis.map { (abi, triple) ->
    tasks.register<CargoBuild>("cargoBuild-$abi") {
        cargo.set(cargoExecutable)
        this.abi.set(abi)
        target.set(triple)
        linker.set(androidComponents.sdkComponents.ndkDirectory.map { ndk ->
            val suffix = if (OperatingSystem.current().isWindows) ".cmd" else ""
            ndk.file("toolchains/llvm/prebuilt/$hostTag/bin/${clangPrefix.getValue(triple)}$minimumSdk-clang$suffix").asFile.absolutePath
        })
        workspace.set(workspaceDirectory)
        sources.from(workspaceDirectory.dir("crates"), workspaceDirectory.file("Cargo.toml"), workspaceDirectory.file("Cargo.lock"))
        outputDirectory.set(layout.buildDirectory.dir("rust/$abi"))
    }
}

androidComponents {
    onVariants { variant ->
        cargoTasks.forEach { task -> variant.sources.jniLibs?.addGeneratedSourceDirectory(task, CargoBuild::outputDirectory) }
    }
}

dependencies {
    implementation(libs.androidx.core)
    implementation(libs.androidx.activity.compose)
    implementation(libs.androidx.lifecycle.viewmodel.compose)
    implementation(libs.androidx.lifecycle.runtime.compose)
    implementation(platform(libs.compose.bom))
    implementation(libs.compose.ui)
    implementation(libs.compose.material3)
    implementation(libs.compose.material.icons)
    implementation(libs.compose.ui.tooling.preview)
    implementation(libs.kotlinx.serialization.json)
    implementation(libs.okhttp)
    implementation(libs.maplibre)
    implementation(libs.car.app)
    testImplementation(libs.junit)
}

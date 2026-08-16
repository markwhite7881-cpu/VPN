import java.io.File
import org.apache.tools.ant.taskdefs.condition.Os
import org.gradle.api.DefaultTask
import org.gradle.api.GradleException
import org.gradle.api.logging.LogLevel
import org.gradle.api.tasks.Input
import org.gradle.api.tasks.TaskAction

open class BuildTask : DefaultTask() {
    @Input
    var rootDirRel: String? = null
    @Input
    var target: String? = null
    @Input
    var release: Boolean? = null

    @TaskAction
    fun assemble() {
        val executable = """npm""";
        try {
            runTauriCli(executable)
        } catch (e: Exception) {
            if (Os.isFamily(Os.FAMILY_WINDOWS)) {
                // Try different Windows-specific extensions
                val fallbacks = listOf(
                    "$executable.exe",
                    "$executable.cmd",
                    "$executable.bat",
                )

                var lastException: Exception = e
                for (fallback in fallbacks) {
                    try {
                        runTauriCli(fallback)
                        return
                    } catch (fallbackException: Exception) {
                        lastException = fallbackException
                    }
                }
                throw lastException
            } else {
                throw e;
            }
        }
    }

    fun runTauriCli(executable: String) {
        val rootDirRel = rootDirRel ?: throw GradleException("rootDirRel cannot be null")
        val target = target ?: throw GradleException("target cannot be null")
        val release = release ?: throw GradleException("release cannot be null")
        val cliArgs = mutableListOf("run", "--", "tauri", "android", "android-studio-script");

        if (project.logger.isEnabled(LogLevel.DEBUG)) {
            cliArgs.add("-vv")
        } else if (project.logger.isEnabled(LogLevel.INFO)) {
            cliArgs.add("-v")
        }
        if (release) {
            cliArgs.add("--release")
        }
        cliArgs.addAll(listOf("--target", target))

        project.exec {
            workingDir(File(project.projectDir, rootDirRel))
            if (Os.isFamily(Os.FAMILY_WINDOWS)) {
                // CreateProcess cannot resolve/execute batch shims (npm.cmd) from
                // a working directory with non-ASCII characters; go through cmd.exe.
                executable("cmd.exe")
                args(listOf("/c", executable) + cliArgs)
            } else {
                executable(executable)
                args(cliArgs)
            }
        }.assertNormalExitValue()
    }
}
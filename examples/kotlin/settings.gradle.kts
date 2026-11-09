rootProject.name = "lavi-example-kotlin"

// Local monorepo composite-build substitution (omit in standalone projects
// that resolve `io.github.emmby:lavi` directly from Maven Central):
includeBuild("../../lavi-jvm") {
    dependencySubstitution {
        substitute(module("io.github.emmby:lavi")).using(project(":"))
    }
}

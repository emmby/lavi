rootProject.name = "lavi-example-kotlin"

includeBuild("../../lavi-jvm") {
    dependencySubstitution {
        substitute(module("io.github.emmby:lavi")).using(project(":"))
    }
}

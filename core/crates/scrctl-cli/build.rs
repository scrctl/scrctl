fn main() {
    let cmake = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../CMakeLists.txt");
    println!("cargo:rerun-if-changed={}", cmake.display());
    let source = std::fs::read_to_string(cmake).expect("read scrctl project metadata");
    let version = source
        .split_once("project(scrctl")
        .and_then(|(_, project)| project.split_once("VERSION"))
        .and_then(|(_, version)| version.split_whitespace().next())
        .expect("scrctl project version");
    println!("cargo:rustc-env=SCRCTL_PUBLIC_VERSION={version}");
}

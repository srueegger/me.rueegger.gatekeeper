cmake_minimum_required(VERSION 3.21)

project(Gatekeeper
    VERSION 0.1.0
    LANGUAGES CXX
)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(Qt6 6.8 REQUIRED COMPONENTS Quick QuickControls2)

qt_standard_project_setup(REQUIRES 6.8)

# --------------------------------------------------------------------------------------
# Rust-Kern
#
# Bewusst ohne Corrosion: dessen cxx-Unterstützung setzt das Kommandozeilenwerkzeug
# `cxxbridge` voraus, das im Offline-Build von Flatpak eigens mitgeliefert werden müsste.
# `cxx_build` in build.rs erzeugt Header und Glue-Code innerhalb von Cargo, womit hier nur
# noch der Aufruf und das Verlinken bleiben. Siehe ADR-7.
# --------------------------------------------------------------------------------------

find_program(CARGO_EXECUTABLE cargo REQUIRED)

set(GK_CARGO_TARGET_DIR "${CMAKE_BINARY_DIR}/cargo")

# Cargo legt das Profil `dev` unter `debug/` ab, alle anderen unter ihrem eigenen Namen.
if(CMAKE_BUILD_TYPE STREQUAL "Debug")
    set(GK_CARGO_PROFILE dev)
    set(GK_CARGO_PROFILE_DIR debug)
else()
    set(GK_CARGO_PROFILE release)
    set(GK_CARGO_PROFILE_DIR release)
endif()

# Im Flatpak-Build liegen die Abhängigkeiten vendored vor und es gibt kein Netz.
option(GK_CARGO_OFFLINE "cargo ohne Netzwerkzugriff aufrufen" OFF)
if(GK_CARGO_OFFLINE)
    set(GK_CARGO_NET_FLAGS --offline --locked)
else()
    set(GK_CARGO_NET_FLAGS "")
endif()

set(GK_RUST_STATICLIB "${GK_CARGO_TARGET_DIR}/${GK_CARGO_PROFILE_DIR}/libgatekeeper_ffi.a")
set(GK_CXXBRIDGE_INCLUDE_DIR "${GK_CARGO_TARGET_DIR}/cxxbridge")

file(GLOB_RECURSE GK_RUST_SOURCES CONFIGURE_DEPENDS
    "${CMAKE_SOURCE_DIR}/crates/*.rs"
    "${CMAKE_SOURCE_DIR}/crates/*/Cargo.toml"
)

add_custom_command(
    OUTPUT
        "${GK_RUST_STATICLIB}"
    COMMAND
        ${CMAKE_COMMAND} -E env "CARGO_TARGET_DIR=${GK_CARGO_TARGET_DIR}"
        "${CARGO_EXECUTABLE}" build --package gatekeeper-ffi
        --profile ${GK_CARGO_PROFILE} ${GK_CARGO_NET_FLAGS}
    DEPENDS
        ${GK_RUST_SOURCES}
        "${CMAKE_SOURCE_DIR}/Cargo.toml"
        "${CMAKE_SOURCE_DIR}/Cargo.lock"
    WORKING_DIRECTORY
        "${CMAKE_SOURCE_DIR}"
    COMMENT
        "Rust-Kern bauen (Profil ${GK_CARGO_PROFILE})"
    VERBATIM
)

add_custom_target(gatekeeper_rust
    DEPENDS "${GK_RUST_STATICLIB}"
)

# Die von cxx erzeugten Header entstehen erst beim Cargo-Lauf. Ohne dieses Verzeichnis
# schlägt die Konfiguration fehl, bevor es jemals angelegt werden konnte.
file(MAKE_DIRECTORY "${GK_CXXBRIDGE_INCLUDE_DIR}")

add_library(gatekeeper_ffi INTERFACE)
target_include_directories(gatekeeper_ffi INTERFACE "${GK_CXXBRIDGE_INCLUDE_DIR}")
# Eine Rust-staticlib bringt ihre Systemabhängigkeiten nicht selbst mit.
target_link_libraries(gatekeeper_ffi INTERFACE "${GK_RUST_STATICLIB}" pthread dl m)

# --------------------------------------------------------------------------------------
# Anwendung
# --------------------------------------------------------------------------------------

qt_add_executable(gatekeeper
    src/main.cpp
    src/Session.h
    src/Session.cpp
)

# Main.qml liegt unter src/qml/, muss in den Ressourcen aber direkt unter dem Modulpfad
# stehen, damit loadFromModule() es findet.
set_source_files_properties(src/qml/Main.qml PROPERTIES
    QT_RESOURCE_ALIAS Main.qml
)

qt_add_qml_module(gatekeeper
    URI GatekeeperUi
    QML_FILES
        src/qml/Main.qml
    # Der Import-Scan dient statischen Qt-Builds und Qts Deployment-Werkzeugen. Wir nutzen
    # beides nicht: die QML-Module kommen aus der KDE-Runtime des Flatpaks und werden zur
    # Laufzeit geladen. Debian exportiert für seine Plugins ohnehin keine CMake-Ziele,
    # weshalb der Scan sonst nur unvermeidbare Warnungen erzeugt.
    NO_IMPORT_SCAN
)

add_dependencies(gatekeeper gatekeeper_rust)

# Der von Qt erzeugte Registrierungscode inkludiert <Session.h> mit spitzen Klammern.
target_include_directories(gatekeeper PRIVATE src)

target_link_libraries(gatekeeper PRIVATE
    Qt6::Quick
    Qt6::QuickControls2
    gatekeeper_ffi
)

# --------------------------------------------------------------------------------------
# Installation
# --------------------------------------------------------------------------------------

install(TARGETS gatekeeper
    RUNTIME DESTINATION "${CMAKE_INSTALL_BINDIR}"
)

install(FILES data/me.rueegger.Gatekeeper.desktop
    DESTINATION "${CMAKE_INSTALL_DATADIR}/applications"
)

install(FILES data/me.rueegger.Gatekeeper.metainfo.xml
    DESTINATION "${CMAKE_INSTALL_DATADIR}/metainfo"
)

install(FILES data/icons/hicolor/scalable/apps/me.rueegger.Gatekeeper.svg
    DESTINATION "${CMAKE_INSTALL_DATADIR}/icons/hicolor/scalable/apps"
)

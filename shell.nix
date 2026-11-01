{
  pkgs ? import (builtins.getFlake "nixpkgs").outPath { },
}:
let
  qt = pkgs.qt6.env "dictate-qt-env" (
    with pkgs;
    [
      qt6.qtdeclarative
      qt6.qtwayland
      kdePackages.layer-shell-qt
      kdePackages.qqc2-desktop-style
    ]
  );
in
pkgs.mkShell {
  packages = with pkgs; [
    pkg-config
    just
    ruff
    clang
    qt6.qtbase
    qt6.qtdeclarative
    qt6.qtwayland
    kdePackages.layer-shell-qt
    kdePackages.qqc2-desktop-style
    xorg-server
    xvfb-run
    xdotool
    xwininfo
    xprop
    imagemagick
    dbus
  ];
  shellHook = ''
    # CXX-Qt discovers headers, libraries and .prl files through qmake.
    # Present Qt's separate Nix outputs as one installation.
    export QMAKE="${qt}/bin/qmake"
    export QT_PLUGIN_PATH="${qt}/${pkgs.qt6.qtbase.qtPluginPrefix}:/run/current-system/sw/lib/qt-6/plugins''${QT_PLUGIN_PATH:+:$QT_PLUGIN_PATH}"
    export QML_IMPORT_PATH="${qt}/${pkgs.qt6.qtbase.qtQmlPrefix}:${pkgs.kdePackages.kirigami.unwrapped}/${pkgs.qt6.qtbase.qtQmlPrefix}:${pkgs.kdePackages.sonnet}/${pkgs.qt6.qtbase.qtQmlPrefix}''${QML_IMPORT_PATH:+:$QML_IMPORT_PATH}"
    export QML2_IMPORT_PATH="$QML_IMPORT_PATH"
    export LD_LIBRARY_PATH="${
      pkgs.lib.makeLibraryPath [
        qt
        pkgs.libx11
        pkgs.libxkbcommon
        pkgs.stdenv.cc.cc.lib
      ]
    }''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
  '';
}

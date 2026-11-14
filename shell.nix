# Dev shell for NixOS: `nix-shell`, then `cargo build` / `cd app && npm run tauri dev`.
{ pkgs ? import <nixpkgs> { } }:

pkgs.mkShell {
  nativeBuildInputs = with pkgs; [
    pkg-config
    nodejs
  ];

  buildInputs = with pkgs; [
    # enigo's Wayland backend (CLI and app)
    libxkbcommon
    # Tauri / WebKitGTK (app only)
    webkitgtk_4_1
    libsoup_3
    gtk3
    openssl
    librsvg
  ];
}

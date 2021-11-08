{
  inputs = {
    nixpkgs = { url = github:nixos/nixpkgs; };
    utils.url = github:numtide/flake-utils;
  };

  outputs = { self, nixpkgs, utils }:
    utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
      in {
        devShell = pkgs.mkShell {
          name = "temu-shell";
          nativeBuildInputs = with pkgs; [
            rustc
            cargo
            pkg-config
          ];
          buildInputs = with pkgs; [
            libGL
            wayland.dev
            libxkbcommon
            vulkan-loader
            freetype
            fontconfig
          ];
        };
    });
}


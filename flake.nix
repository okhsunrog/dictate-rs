{
  description = "Development environment for Dictate";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  outputs = { nixpkgs, ... }: {
    devShells.x86_64-linux.default = import ./shell.nix {
      pkgs = import nixpkgs { system = "x86_64-linux"; };
    };
  };
}

{
  pkgs ? import <nixpkgs> { },
}:
pkgs.mkShell {
  shellHook = ''
    export LIBCLANG_PATH="${pkgs.libclang.lib}/lib"
  '';

  nativeBuildInputs = with pkgs; [
    pkg-config
  ];

  buildInputs = with pkgs; [
    glib
    clang
    openssl
    libclang
  ];
}

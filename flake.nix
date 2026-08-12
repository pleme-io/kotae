{
  description = "Kotae (答え) — the typed answer a machine gives a machine: found / empty / refused / blind, rendered so no two can produce the same bytes";

  inputs.substrate.url = "github:pleme-io/substrate";

  outputs =
    { substrate, ... }:
    substrate.rust.library {
      src = ./.;
    };
}

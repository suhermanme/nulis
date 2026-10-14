# NvChad Base46 theme snapshot

The 97 `.lua` files here were downloaded from [NvChad/base46](https://github.com/NvChad/base46/tree/v3.0/lua/base46/themes), branch `v3.0`, commit `7df2bd89295db00b649cbc49bdf6fe5a1cc5df3b` (2026-10-13 snapshot). The upstream `LICENSE` and `base16-LICENSE` are included unchanged.

Nulis does not execute these files. The bundled application reads the converted `themes/nvchad-catalog.json`. To regenerate that catalog after updating the source snapshot, run from the repository root:

```sh
cargo run -p nulis-theme --example import_base46 -- themes/nvchad-source themes/nvchad-catalog.json
```

The theme crate test checks that all source files still convert to the committed catalog.

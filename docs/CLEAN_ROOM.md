# Clean-room policy

## Purpose

Nulis is an original editor in the professional text/code/hex editor category. It may implement publicly described capabilities and standards, but its implementation, UX, text, assets, and branding must be independently created.

## Permitted inputs

- Public product documentation and public feature descriptions used only to identify functional requirements.
- Public standards, protocols, file formats, and accessibility guidance.
- Behavior observed through ordinary use and restated as generic requirements.
- Open-source libraries whose licenses are compatible with Nulis and recorded in `THIRD_PARTY_LICENSES.md`.

## Prohibited inputs and actions

Contributors must not decompile, disassemble, debug, inspect packaged resources, extract, patch, or reverse-engineer proprietary editor binaries. They must not use leaked or non-public information, copy proprietary code, strings, documentation wording, configuration, icons, themes, layouts, screenshots, artwork, or reproduce another product pixel-for-pixel. Nulis must neither require nor inspect an UltraEdit installation.

## Contribution procedure

1. Describe behavior in product-neutral language in `PRODUCT_SPEC.md` or an issue.
2. Design the implementation from Nulis architecture and public standards.
3. Record new dependencies and licenses.
4. Create original UI and assets and test against Nulis requirements.
5. Reject or rewrite contributions with unclear provenance.

Third-party source reuse must retain required notices and must be visibly attributed. Product comparisons belong only in private evaluation notes or generic parity tracking, never as copied documentation.

## Compliance record

The initial repository was created from the project specification in this repository, public Tauri/React/Rust interfaces, and original code. No proprietary product binary, source, asset, configuration, or installation is used.


# Pure wasm plugin values

## Background

Pure wasm plugin values — revisit D06 ("plugins cannot inject logic") for a plugin-shipped
wasm module with no imports, called by the core at render time, for what a template must
compute on and a mark cannot carry (D42): locale formatting, plurals, collation.
Output-producing extensions — fence renderers, Pikchr, TeX → MathML — are `code` producers
instead and need nothing here

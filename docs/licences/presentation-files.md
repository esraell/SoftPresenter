# Presentation files (.pptx)

SoftPresenter reads a `.pptx` file directly into slides (`crates/sp-app/src/pptx.rs`). The file
is a zip of XML parts. The format is public (ECMA-376, Office Open XML); nothing is taken from
any program's code, and no program is needed.

| Crate | Licence |
|---|---|
| roxmltree 0.21 (std only) | MIT OR Apache-2.0 |
| zip 9 (deflate only) | MIT |

Both were already in the build (roxmltree through the font and picture libraries); reading
presentations adds no new crate, only a direct use of them.

## What is not trusted

A `.pptx` may come from anyone.

- The zip is read through `sp_model::bounded_zip`, the same code that reads zipped Bibles:
  the zip's size, the number of entries, each entry's unpacked size and all of them together
  are bounded by what is actually read, never by what an entry says about itself; a zip whose
  entries overlap is refused; an entry's name is a key into a table and never a place on
  disk; a zip inside a zip is not opened.
- XML is read by roxmltree with document type declarations refused, so an entity is never
  expanded and nothing a file names is fetched; nesting deeper than 64 elements or more than 64
  attributes on one element refuses the part.
- A picture over 32 MB, or with more than 64 million pixels, is left out and its slide is named
  in the import's notice; a picture is only copied into the Images folder under a name the app
  makes.

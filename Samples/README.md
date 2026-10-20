# Highlighting samples

One file per language Nulis highlights, each at least 100 lines, for checking syntax colors, folding, and the built-in tools by eye. The content is made up and is not meant to run.

The JSON, XML, XSL, and plist samples are valid, so Validate and Beautify work on them. `sample.yaml` uses an anchor and `<<: *defaults`, so YAML Beautify asks before writing it out in full. `sample.csv` has quoted fields with commas, doubled quotes, and line breaks. `sample-binary.plist` is a binary property list, which opens read-only in the hex editor.

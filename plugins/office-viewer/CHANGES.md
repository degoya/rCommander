# Changes

## 0.1.1

Built with quick-xml 0.42, which checks every part of a document for valid UTF-8: a part with a
stray byte that is not UTF-8 - in a tag, an attribute or a comment, not only in the text - is now
reported as damaged instead of being shown with that piece left out. Office always writes UTF-8,
so a document Word, Excel or PowerPoint saved shows as before. Built against plugin contract
0.6.0.

## 0.1.0

The first release: the text of Word documents, workbooks and presentations (`docx`, `docm`,
`xlsx`, `xlsm`, `pptx`, `pptm`) - paragraphs and tables, sheets with their values and the last
saved results of their formulas, slides in the order of the presentation - and their properties
with the sheets or slides as a tree. How many rows a sheet shows is set on the Plugins page.
Macros, links outside the document and oversized or overcompressed parts are never read.

Built against plugin contract 0.4.0.

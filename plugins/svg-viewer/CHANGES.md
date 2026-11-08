# Changes

## 0.1.6

A file the viewer refuses (too large, a damaged `.svgz`, not an SVG, too complex) shows its
explanation again: the failure codes carry the plugin's id, as its texts do. Built against plugin
contract 0.6.0.

## 0.1.5

Two more ways a small file kept the plugin busy end with "too complex" at once. An arc with a
huge radius, which the renderer would cut into hundreds of thousands of curves, is refused, and
so are arcs that together make more than a million curves; an ordinary arc makes a handful. And
the canvases the renderer draws groups with an opacity, a filter, a mask or a clip path into are
counted before anything is drawn: nested or repeated beyond 64 million pixels, the picture is
refused instead of running out of memory.

Built against plugin contract 0.4.0.

## 0.1.4

A DOCTYPE no longer expands entities: the usual `<!DOCTYPE svg PUBLIC ...>` line is ignored and
the picture draws, while a picture that uses an entity its DOCTYPE declares shows an error instead
of the picture. A few kilobytes of such entities could fill the plugin's memory before. The
nesting limit is checked once more on the parsed document, and the parser stops at the element
limit instead of building everything first.

## 0.1.3

The limit against SVG bombs counts what the renderer really builds. An SVG embedded as a `data:`
picture now counts against it each time it is drawn, an id defined twice is taken at its first
element, and a reference with spaces around `#id` is followed. Any of these let a small file make
the renderer build billions of elements before. Markers count once for every vertex they are
drawn on, patterns once for every shape painted with them, nested ones included, and the canvases
patterns are drawn into have a limit of their own; a ring of markers or patterns that refer to
each other is refused.

## 0.1.2

Drawn with resvg 0.49: an SVG that names its link both as `href` and `xlink:href` follows
`href`, as SVG 2 asks. An SVG embedded as a `data:` picture still draws no picture of its own.

## 0.1.1

Built against plugin contract 0.3.0; nothing else changed.

## 0.1.0

The first release: SVG and SVGZ drawn as a picture at 512, 1024 or 2048 pixels on a transparent,
white or black background, text in the bundled Lato font, and the source coloured as XML. Nothing
outside the file is loaded.

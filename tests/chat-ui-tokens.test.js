// DROPPED (#1268): "every colour, font and radius outside a design block reads a token"
// scans CSS text for raw colors outside a design block.
// DROPPED (#1268): "a design block is skipped however it is indented"
// feeds a CSS string the test wrote into the same scanner.
// DROPPED (#1268): "a literal outside a design block is caught"
// feeds another CSS string the test wrote and expects the scanner to flag it.

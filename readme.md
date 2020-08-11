# cap-parser

This program can read the [Presentation Graphics Stream (PGS)](http://blog.thescorpius.com/index.php/2017/07/15/presentation-graphic-stream-sup-files-bluray-subtitle-format/) and output
the text represented by the subtitle images from a BluRay DVD rip.

This program requires tesserract OCR to be installed on your system, and uses it to perform the conversion from image to text.

The subtitles can be written in the .srt format, which includes text information, and timestamp information.

This is a multi-threaded implementation which can convert a 2.5 hour movie from bitmap subtitles to text in 15 seconds on my Ryzen 3950x, and
scaling is essentially linear with processing power. The bottleneck is the OCR, as parsing and preparing the images takes an insignificant amount of time.
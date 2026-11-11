# EditorTool V1.0

Der erste vollständige Release von EditorTool: ein schlanker nativer Rust-Editor für Text, Code und Bilder.

## Neu

- Kompakter Darkmode mit mehreren Fenstern und Tabs.
- Tabs oben oder links; die vertikale Seitenleiste lässt sich per Ziehen verbreitern.
- Automatische Tab-Namen aus Dateinamen oder den ersten Wörtern des Texts.
- Bilder direkt im Textfluss einfügen: **Bearbeiten → Bild einfügen** oder **Strg/Cmd+Umschalt+I**; Bilder aus der Zwischenablage auch über **Strg/Cmd+V**.
- Text und Bilder gemeinsam als `.etdoc` speichern. Reine Text- und Code-Dateien bleiben normale Dateien.
- Sitzungscache für offene Fenster, Tabs, ungespeicherte Inhalte, Bilder und Cursorpositionen.
- Syntaxfarben, Zeilennummern, Auswahlstatistik, Suchen/Ersetzen und Textvergleich.
- Kompakte, leicht transparente Suche über **Strg/Cmd+F**.

## Downloads

Für jede Plattform stehen **x64 und ARM64** zur Verfügung:

| System | Installation |
| --- | --- |
| Windows | `*-setup.exe`; erscheint unter „Installierte Apps“ und bei den Standard-Apps für `.etdoc` |
| Fedora / Nobara | `.rpm` oder `.flatpak` |
| Ubuntu / Debian | `.deb` oder `.flatpak` |
| Weitere Linux-Distributionen | `.flatpak` oder portables `.tar.gz` |
| macOS Intel / Apple Silicon | `.dmg`; App in den Programme-Ordner ziehen |

Zusätzlich enthalten die Releases portable ZIP-Archive und `SHA256SUMS.txt`.

## Hinweise

- Das Repository und alle Downloads sind öffentlich zugänglich.
- Die Pakete sind nicht mit einem Windows-/Apple-Entwicklerzertifikat signiert. macOS-Bundles erhalten eine lokale Ad-hoc-Signatur; sie sind nicht notarisiert.
- Die Builds und Installationspakete werden auf GitHub erstellt. Auf Wunsch wurden keine Test-Suites ausgeführt.
- Syntaxfarben sind lexikalisch; es gibt keine Code-Ausführung oder Sprachprüfung. Über 4 MiB pro Textbereich bleibt die Färbung aus.
- Der Vergleich vergleicht Text. Bilder werden eingebettet gespeichert, aber nicht visuell verglichen.
- In Dokumenten mit Bildern stehen Text und Bilder untereinander. Bilder lassen sich per Doppelklick nach Rückfrage entfernen; Textabschnitte besitzen ihren eigenen Undo-Verlauf.
- Bilder erhöhen Speicherbedarf und Cache-Größe. Eingefügte Bilder werden als PNG eingebettet, maximal 40 Megapixel pro Bild.
- Bei einem erzwungenen Abbruch kann nur der zuletzt abgeschlossene Cache-Stand wiederhergestellt werden.

## Lizenz

**The Unlicense** für die eigenen Projektdateien. Die Lizenzen der Drittanbieter-Bibliotheken sind separat beigefügt.

# EditorTool

Ein schlanker nativer Rust-Editor für **Text, Code und Bilder** — für Windows, Linux und macOS, jeweils **x64 und ARM64**.

[Downloads](https://github.com/OnlySecci/EditorTool/releases) · [Release Notes](RELEASE_NOTES.md) · [Changelog](CHANGELOG.md) · [Entwicklung](CONTRIBUTING.md)

![EditorTool mit kompakter dunkler Oberfläche und vertikalen Tabs](docs/images/editor-main.png)

*Hauptansicht aus V1.0 — vom Projektinhaber bereitgestellte Aufnahme.*

## Neu in V1.4.5

Sync-Änderungen verwenden vorhandene Bild- und Textansichten weiter und erhalten die sichtbare Scrollposition. Eine feste Umbruchbreite verhindert vorübergehende Layout-Sprünge bei gemischten Dokumenten. Neue oder entfernte Bilder werden ohne Zwischenansicht übernommen.

Synchronisierte Dokumente zeigen unten einen **Countdown der fünf Sekunden Schreibpause**. Anschliessend stehen dort Freigabe-/Abgleichstatus, Offline-Warten, Pause, Konflikte oder die Bestätigung der aktuellen Revision durch die aktiven Clients. Der Countdown zeigt die lokale Schreibpause; die anschliessende Übertragung kann länger dauern. Ein Sekunden-Timer läuft nur während einer sichtbaren Schreibpause.

Das **Tab-Kontextmenü** verwendet abgerundete Zeilen, Akzentfarbe, Gruppen und eigene Unterseiten für Sprache und Split View. Maus und Pfeiltasten wählen, Enter bestätigt, Escape oder ein Klick ausserhalb schliesst. Details stehen in den [Release Notes](RELEASE_NOTES.md).

## Neu in V1.4.1

**Einstellungen** steht neben **Bearbeiten** und öffnet **Allgemein** beziehungsweise **Clients und Sync** als Tabs in der Datei-Liste. Client-Name, IP-Adresse und Ports sind sichtbar beschriftet; Freigaben verwenden die Gestaltung der App. Der Textcursor blinkt. **Ansicht → Zoom**, **Strg/Cmd + Plus/Minus/0** und **Strg/Cmd + Mausrad** zoomen Textansichten zwischen 50 und 300 %. Zoom bleibt im Sitzungscache gespeichert. Bildabstände werden anhand der nativen Schriftmetriken berechnet, auch auf macOS.

Senden und Sync verwenden jetzt verpflichtend **TLS 1.3 + AES-256-GCM** mit hybridem Schlüsselaustausch **X25519 + ML-KEM-768**. Die Einstellungen zeigen den Schutz an. Gerätesignaturen und Fingerabdruckfreigaben bleiben klassisch; die Hybridtechnik schützt den Schlüsselaustausch. **Alle beteiligten Geräte benötigen V1.4.1 oder neuer**, da kein stiller Rückfall auf klassische Verbindungen erlaubt ist. Geräteidentitäten und lokale Daten bleiben erhalten. [Technische Hintergründe bei Rustls](https://rustls.dev/docs/rustls/manual/_05_defaults/index.html).

**Sync beenden** benachrichtigt die anderen Teilnehmer und beendet dort die Verbindung für diesen Client und dieses Dokument. Bei Gruppen bleiben die übrigen Teilnehmer verbunden. Offline-Abmeldungen werden nachgeholt, lokale Kopien bleiben erhalten. Alle Änderungen stehen in den [Release Notes](RELEASE_NOTES.md).

## Neu in V1.4: Senden und Sync

Unter **Einstellungen → Clients, Senden und Sync** interne IP-Adresse und Port anderer Geräte hinzufügen. Standard ist **TCP 51473**; der eigene Port und Client-Ports sind änderbar. Erst **Senden und Sync aktivieren** öffnet den Freigabe-Port. Verschlüsseltes gegenseitiges TLS mit beidseitig bestätigten Fingerabdrücken schützt die Verbindung. **Connected / Not connected** zeigt die Erreichbarkeit. Interne geroutete Netze und VPN sind möglich; öffentliche IP-Adressen werden abgelehnt. Firewalls müssen den gewählten TCP-Port erlauben. EditorTool ändert keine Firewall-Regeln.

Über das **Tab-Kontextmenü** eine Datei an genau einen Client senden oder mit mehreren Clients synchronisieren. Jeder Empfänger nimmt die Anfrage selbst an oder lehnt sie ab. Text, Code und Bilder werden zusammen übertragen; lokale Pfade und rechte Vergleichstexte bleiben lokal. Änderungen werden nach **fünf Sekunden Schreibpause** bereitgestellt und im Hintergrund abgeglichen. Bei erneuter Erreichbarkeit werden Versionen nachgeholt. Versionszähler pro Gerät und Inhaltsprüfsummen erkennen konkurrierende Änderungen ohne Abhängigkeit von PC-Uhrzeiten. **Sync-Versionen / Konflikte** bietet Vorschau und manuelle Versionsauswahl; abweichende Inhalte bleiben im lokalen Versionsspeicher erhalten.

**Automatisch in lokale Datei speichern** ist pro Dokument wählbar. Externe Dateiänderungen pausieren das automatische Speichern; bestehende Dateien werden dann nicht überschrieben. **Beim Schliessen im Tray weiterlaufen** ist separat wählbar, standardmässig aus. Ohne Linux-System-Tray bleibt das Fenster geöffnet; GNOME benötigt eventuell eine StatusNotifier-/AppIndicator-Erweiterung. Es gibt keinen installierten Systemdienst und keine Cloud.

Maximal 64 MiB je Dokument und 32 Geräte je Sync-Gruppe. Geänderte Teilnehmerlisten starten eine neue Gruppe mit neuen Freigaben. Im lokalen `sharing`-Ordner liegen Geräteidentität, Anfragen und Inhaltsversionen unverschlüsselt auf der Festplatte; historische Inhalte werden nicht automatisch gelöscht. Details und Einrichtung stehen in den [Release Notes](RELEASE_NOTES.md).

## Neu in V1.3: Bilder und Split View

Bilder anklicken und an einer Ecke proportional kleiner oder grösser ziehen. Standardmässig werden neu eingefügte Bilder auf höchstens 50 % der Textfeldbreite verkleinert; unter Einstellungen lässt sich dies abschalten. Die Anzeigegrösse wird gespeichert, die Originalbilddaten bleiben erhalten. Per Rechtsklick lassen sich Bilder an das Textfeld anpassen oder entfernen.

Rechtsklick auf einen Dokument-Tab öffnet Speichern, Speichern unter, Sprache, Dateipfad und Tab-Aktionen. Unter **Ansicht → Split View** lassen sich zwei unabhängige Ansichten **nebeneinander oder übereinander** öffnen. Jede hat ihre eigene Datei-/Tab-Liste; den Trenner kann man ziehen. Über das Tab-Menü lassen sich Dateien auf die andere Seite verschieben. **Als zwei Fenster weiterführen** löst die Teilung auf, ohne Dokumente oder Undo-Verläufe zu verwerfen. Die Sitzung speichert beide Ansichten und ihre Anordnung.

## Neu in V1.2: Updates

Unter **Hilfe → Nach Updates suchen** werden neue stabile GitHub-Versionen und deren Release Notes angezeigt. **Jetzt aktualisieren** lädt das passende Paket, prüft SHA-256, sichert die Sitzung und startet Installation und App-Neustart. In den Einstellungen: Startprüfung an/aus, automatische Installation an/aus. Standard: Startprüfung an, automatische Installation aus. Die manuelle Prüfung funktioniert unabhängig davon.

V1.2 einmal manuell installieren; danach lassen sich zukünftige Versionen aus der App aktualisieren. Windows unterstützt Setup und portable Installationen, Linux RPM/DEB/Flatpak und portable Programmdateien, macOS das App-Bundle. Systempakete oder geschützte App-Ordner können eine Berechtigungsabfrage des Betriebssystems auslösen. Flatpak erlaubt Netzwerkzugriff und einen Host-Aufruf für die Paketinstallation. Es wird nur beim Start beziehungsweise auf Wunsch geprüft; die Update-Suche hat keinen ständig laufenden Dienst. Der optionale Tray-Modus gehört zur internen Dokumentfreigabe.

Update-Pakete und Protokolle liegen unter `updates` im Einstellungsordner. macOS behält eine versteckte Sicherung des bisherigen App-Bundles im gleichen Ordner. Bei einem Fehler wird die bisherige App nach Möglichkeit erneut geöffnet; beim nächsten Start erscheint ein Hinweis. Prüfsummen ersetzen keine Herausgebersignatur.

## Neu in V1.1

Zeilenumbruch, Farbschema (Dunkel/Hell/Graphit), Akzentfarbe und Textgrösse lassen sich in den Einstellungen ändern. **Datei → Alle Tabs schliessen** schliesst nach Speicherrückfragen alle Tabs des Fensters. Bilder lassen sich seit V1.3 direkt an ihren Ecken verkleinern und vergrössern; per Rechtsklick Bildaktionen öffnen. Die Bildgrösse bleibt im Dokument und im Sitzungscache erhalten. Nach dem Einfügen bleibt das Bild sichtbar.

## Funktionen

- Mehrere Fenster und Tabs; Tab-Leiste oben oder links. Die vertikale Leiste lässt sich am Trennstrich verbreitern. Ihre Breite bleibt gespeichert.
- Gespeicherte Tabs heißen wie die Datei. Ungespeicherte Tabs verwenden die ersten sechs Wörter; leere Tabs heißen „Text 1“, „Text 2“ usw.
- Text und Bilder untereinander. Bilder über **Bearbeiten → Bild einfügen**, **Strg/Cmd+Umschalt+I** oder die Bild-Zwischenablage einfügen.
- Text und eingebettete Bilder gemeinsam als `.etdoc` speichern. Reine Text- und Code-Dateien bleiben normale UTF-8-Dateien.
- Formatfreies Einfügen und Kopieren von Text; fremde Schriftarten, Farben und HTML-/RTF-Stile werden entfernt.
- Zeilennummern und Statusleiste für Cursor, Zeilen, Zeichen, UTF-8-Bytes, Bilder und Auswahl.
- Kompakte, leicht transparente Suche mit aufklappbarem Ersetzen, einzelnen Treffern und „Alle ersetzen“.
- Textvergleich nebeneinander und lexikalische Syntaxfarben, unter anderem für Python, PowerShell, Rust, JavaScript/TypeScript und JSON.
- Automatische Sitzungswiederherstellung für Fenster, Tabs, ungespeicherte Texte, Bilder, Vergleichstexte und Cursorpositionen.

<details><summary>Suchansicht aus der Entwicklung</summary>

![Suchfenster – ältere Entwicklungsaufnahme](docs/images/search-development.png)

Diese bereitgestellte Aufnahme zeigt die Suche vor ihrer Verkleinerung. V1.0 verwendet ein kompakteres Fenster mit 95 % Deckkraft.

</details>

## Installation

Downloads unter [Releases](https://github.com/OnlySecci/EditorTool/releases). Das Repository und die Downloads sind **öffentlich** zugänglich.

| System | Pakete für x64 und ARM64 |
| --- | --- |
| Windows | Setup-EXE und portables ZIP |
| Fedora / Nobara | RPM oder Flatpak |
| Ubuntu / Debian | DEB oder Flatpak |
| Weitere Linux-Distributionen | Flatpak oder portables TAR.GZ |
| macOS Intel / Apple Silicon | DMG oder ZIP mit App-Bundle |

**Windows:** Setup installiert für den aktuellen Benutzer unter `%LOCALAPPDATA%/Programs/EditorTool`. Die App erscheint unter **Einstellungen → Apps → Installierte Apps** und als mögliche Standard-App für `.etdoc`. Der Sitzungscache bleibt bei der Deinstallation erhalten.

**Linux:** RPM beziehungsweise DEB über den Paketmanager installieren. Flatpak: `flatpak install --user ./EditorTool-*.flatpak`, danach `flatpak run com.github.OnlySecci.EditorTool`. Der Runtime-Zugang ist im Bundle hinterlegt; eine Veröffentlichung auf Flathub ist damit nicht verbunden. Die native Linux-Version nutzt X11 beziehungsweise XWayland.

**macOS:** DMG öffnen und die App in „Programme“ ziehen. Intel verwendet `x86_64`, Apple Silicon `arm64`.

Die Pakete sind **ohne Windows-/Apple-Entwicklerzertifikat** veröffentlicht. macOS-App-Bundles erhalten eine lokale Ad-hoc-Signatur und sind nicht notarisiert. `SHA256SUMS.txt` enthält die Prüfsummen.

## Sitzung und Dokumente

Änderungen gehen nach etwa 400 ms in einen Hintergrundschreibvorgang. Beim Schließen eines Fensters wird auf die Sicherung gewartet. Das Fenster wird beim nächsten Start wiederhergestellt; ausdrücklich geschlossene Tabs werden nicht wieder geöffnet. Unter Windows lösen normales Herunterfahren und Abmelden zusätzlich eine Sicherung aus.

Der automatische Cache überschreibt keine Originaldateien. „Speichern“ schreibt den linken Text beziehungsweise sein Text-/Bild-Dokument in die gewählte Datei. Vergleichstexte bleiben im Cache erhalten. Bei einem erzwungenen Abbruch kann nur der zuletzt abgeschlossene Stand wiederhergestellt werden. Undo-Verläufe werden nicht gespeichert.

| Plattform | Einstellungen und Cache |
| --- | --- |
| Windows | `%APPDATA%/EditorTool` |
| Linux | `$XDG_CONFIG_HOME/EditorTool`, sonst `~/.config/EditorTool` |
| macOS | `~/Library/Application Support/EditorTool` |
| Portable-Modus | `Data` neben der EXE, wenn dort `portable.txt` liegt |

Der Unterordner `session` enthält Klartext und eingebettete Bilddaten. Weitere Fenster öffnet man über **Datei → Neues Fenster**. Aus dem Dateimanager geöffnete Dateien werden an die bestehende Instanz übergeben.

## Bedienung

Windows/Linux: **Strg**, macOS: **Cmd**.

| Kürzel | Aktion |
| --- | --- |
| Strg/Cmd+T / +N / +W | Tab erstellen / Fenster erstellen / Tab schließen |
| Strg/Cmd+O / +S / +Umschalt+S | Öffnen / Speichern / Speichern unter |
| Strg/Cmd+F / +H | Suchen / Suchen mit Ersetzen |
| F3 | Nächster Treffer |
| Strg/Cmd+Tab / +Umschalt+Tab | Nächster / vorheriger Tab |
| Strg/Cmd+Umschalt+I | Bild einfügen |
| Strg/Cmd+, | Einstellungen |

Suchen, Ersetzen und **Ansicht → Sprache** arbeiten im zuletzt aktiven Textbereich. Bildaktionen zum Entfernen sind per Rechtsklick erreichbar. Textabschnitte vor und nach Bildern besitzen eigene Undo-Verläufe.

## Selbst bauen

[Rust Stable](https://rustup.rs/) ab Version 1.90 installieren. `Cargo.lock` fixiert die Abhängigkeiten. Erste Builds benötigen Netzwerkzugang. Windows mit MSVC baut FLTK aus Quellen und benötigt zusätzlich CMake, Git und die Visual-C++-Werkzeuge für x64 beziehungsweise ARM64. Dazu `CARGO_BUILD_TARGET` auf `x86_64-pc-windows-msvc` beziehungsweise `aarch64-pc-windows-msvc` setzen.

- Windows: `scripts/build-windows.ps1`, danach `scripts/package-windows.ps1 -PortableOnly`. Für lokale Setup-Builds einen vorhandenen NSIS-Compiler über `NSIS_COMPILER` angeben. GitHub erstellt Setup-EXEs mit dem NSIS-Cross-Compiler auf Linux.
- Linux: C++-, X11-, Pango-, Cairo- und Fontconfig-Entwicklungspakete installieren; `bash scripts/build-linux.sh`, anschließend `package-linux.sh` beziehungsweise `package-flatpak.sh` aus `scripts/` ausführen. Die Paketwerkzeuge müssen vorhanden sein.
- macOS: Xcode Command Line Tools und Rust installieren; `bash scripts/build-macos.sh` und `bash scripts/package-macos.sh` ausführen.

Die GitHub-Matrix baut alle sechs System-/Architektur-Kombinationen. Auf Wunsch werden keine Test-Suites ausgeführt.

## Ressourcen und Grenzen

Rust und FLTK statt Browser-Laufzeit. Die Oberfläche wartet im Leerlauf auf Betriebssystem-Ereignisse. Zähler werden inkrementell aktualisiert; Sitzungssicherung und Syntaxfarben verwenden einmalige Timer nach Änderungen. Konkrete RAM-/CPU-Werte wurden nicht gemessen.

Syntaxfarben sind lexikalisch und bleiben über 4 MiB pro Textbereich aus. Der Textvergleich läuft auf Knopfdruck und wird über 16 MiB gröber; Bilder werden nicht visuell verglichen. Bilder werden als PNG eingebettet, maximal 40 Megapixel pro Bild. Bilder erhöhen RAM- und Cache-Bedarf. Zeichenstatistiken zählen Unicode-Codepoints.

## Lizenz

Die eigenen Projektdateien stehen unter **[The Unlicense](LICENSE)**; der Text liegt auch in [UNLICENSE](UNLICENSE). Drittanbieter behalten ihre eigenen Lizenzen, die unter `third-party-licenses` und in den Paketen mitgeliefert werden.



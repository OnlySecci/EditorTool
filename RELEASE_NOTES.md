# EditorTool V1.4.9

## Fenster und Sitzung

- Zusätzlich geöffnete und danach geschlossene Fenster werden beim nächsten Start nicht mehr automatisch geöffnet. Das zuletzt geschlossene Hauptfenster bleibt als Startsitzung erhalten. Beim Beenden mit mehreren offenen Fenstern bleibt deren Anordnung erhalten.
- **Datei → Geschlossenes Fenster wiederherstellen** holt zusätzliche geschlossene Fenster samt Text, Bildern und Tabs aus dem Cache zurück. Bis zu 20 Ansichten bleiben in dieser Liste; zusammengehörende Split-Ansichten werden gemeinsam behandelt.
- Alte Sitzungscaches unterscheiden offene und geschlossene Fenster noch nicht. Falls beim ersten Start zusätzliche Fenster erscheinen, diese einmal schliessen; danach gilt die neue Wiederherstellung. Im optionalen Tray-Modus versteckt Schliessen weiterhin die laufenden Fenster.

## Flatpak-Updates

- Benutzerinstallationen werden mit `--user` aktualisiert. Für systemweite Installationen ruft der Host-Helfer **Polkit (`pkexec`)** auf, damit der Desktop Administratorrechte abfragen kann. Der bisherige Installationsweg konnte mit „system operation installBundle not allowed for user“ abbrechen.
- Der Neustart verwendet den passenden Benutzer-/Systembereich. Ein Polkit-Authentifizierungsagent auf dem Desktop wird benötigt; EditorTool speichert keine Administratorpasswörter.
- Quellen: [Flatpak-Befehle](https://docs.flatpak.org/en/latest/flatpak-command-reference.html), [Polkit/pkexec](https://www.freedesktop.org/software/polkit/docs/latest/pkexec.1.html).

## Diagnoseprotokoll

- **Hilfe → Diagnoseprotokoll exportieren…** speichert eine lokale `.log`-Datei zum manuellen Anhängen an Problemmeldungen. Kein automatischer Upload.
- Enthalten: Version und Plattform, Fenster-/Sitzungsereignisse, technische Sync-Ereignisse, Verbindungsfehler, Update-Phasen und Fehlerkategorien sowie verfügbare Ausschnitte des Installationsprotokolls.
- Ereignisse enthalten keine Dokumenttexte, Bilder, Zwischenablage, Dateititel, Clientnamen, IP-Adressen, Schlüssel oder Tokens. Eigene Benutzerverzeichnisse im Installationsprotokoll werden durch `<HOME>` ersetzt. Dieses Protokoll kann technische Paketpfade und Betriebssystemmeldungen enthalten; die Exportdatei kann vor dem Teilen geprüft werden.
- Lokale Logs im Unterordner `logs` der Einstellungen haben begrenzte Grösse und Anzahl. Keine Protokollierung pro Tastendruck und kein eigener Hintergrundtimer.

## Dateipfad und Textbearbeitung

- Oberhalb gespeicherter Dokumente steht der Pfad einschliesslich Dateiname. Der Tooltip zeigt auch lange Pfade vollständig; die Zeile lässt sich horizontal bewegen.
- Tab-Kontextmenü: **Dateipfad kopieren** und **Im Dateimanager anzeigen**. Windows/macOS markieren die Datei, Linux öffnet den Ordner.
- **Bearbeiten → Rückgängig / Wiederholen**: Windows/Linux **Strg+Z / Strg+Y**, macOS **Cmd+Z / Cmd+Shift+Z**. Beide Befehle sind unter **Einstellungen → Tastenkürzel** anpassbar und verwenden den Textverlauf im aktiven Textabschnitt. Dieser Verlauf wird nicht über App-Neustarts gespeichert.

## Pakete und Hinweise

Windows Setup-EXE und portables ZIP; Linux Flatpak, RPM, DEB und portables TAR.GZ; macOS DMG und App-ZIP. Jeweils **x64 und ARM64**, dazu `SHA256SUMS.txt`.

Auf Wunsch nur gebaut und paketiert: **keine automatisierten, interaktiven oder Installationstests**. Pakete ohne Entwicklerzertifikate; macOS mit Ad-hoc-Signatur ohne Notarisierung. Projektlizenz: **The Unlicense**, Drittanbieter behalten ihre Lizenzen.

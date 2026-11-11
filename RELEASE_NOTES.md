# EditorTool V1.2

## Neu: integrierte Updates

- **Hilfe → Nach Updates suchen** prüft manuell auf eine neuere stabile GitHub-Version.
- Ein Update-Dialog zeigt die installierte und verfügbare Version sowie die vollständigen Release Notes.
- **Jetzt aktualisieren** lädt das passende Paket für Betriebssystem, Architektur und Installationsart herunter und startet die Installation mit anschliessendem Neustart.
- In den Einstellungen lassen sich die **Update-Prüfung beim Start** und die **automatische Installation beim Start** ein- oder ausschalten. Die Startprüfung ist standardmässig aktiv; automatische Installation ist standardmässig aus. Manuelle Prüfung bleibt verfügbar.
- Downloads werden vor der Installation mit den im Release veröffentlichten SHA-256-Prüfsummen verglichen. Fehlerhafte Downloads werden nicht installiert.
- Vor dem Neustart wird die Sitzung gesichert und auf die Speicherung gewartet. Die App bleibt bei Fehlern vor dem Installationsstart geöffnet. Nach einem Installationsfehler wird nach Möglichkeit die bisherige App erneut geöffnet und beim nächsten Start ein Hinweis angezeigt.

## Pakete

Windows Setup-EXE und portables ZIP, Linux Flatpak/RPM/DEB und portables Archiv, macOS DMG und App-ZIP. Alle Plattformen für x64 und ARM64. SHA256SUMS.txt enthält die Prüfsummen.

Windows nutzt den Setup-Installer oder aktualisiert die portable Installation. Linux verwendet die passende Paketverwaltung, Flatpak oder die portable Programmdatei. macOS ersetzt das App-Bundle und behält eine Sicherung der bisherigen App. Je nach Installationsort kann das Betriebssystem eine Berechtigungsabfrage anzeigen.

Flatpak erhält Netzwerkzugriff für GitHub sowie die Berechtigung, die Flatpak-Installation auf dem Host anzustossen. Beim deaktivierten Startcheck erfolgt keine automatische GitHub-Abfrage. Es läuft kein dauerhafter Hintergrunddienst und keine periodische Update-Abfrage.

## Hinweise

V1.2 muss einmal manuell installiert werden; anschliessend kann der integrierte Updater zukünftige Versionen installieren. Alle Verbesserungen aus V1.1 sind enthalten.

Auf Wunsch wurden keine automatisierten oder interaktiven Tests ausgeführt. Die Pakete sind nicht mit Entwicklerzertifikaten signiert; macOS verwendet eine Ad-hoc-Signatur ohne Notarisierung. SHA-256-Prüfsummen ersetzen keine Herausgebersignatur. Lizenz: The Unlicense für Projektdateien; Drittanbieter behalten ihre Lizenzen.

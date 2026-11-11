# EditorTool V1.4.8

## Menüs und Tastenkürzel

- Die Menü-Reihenfolge lautet jetzt **Einstellungen → Datei → Bearbeiten → Ansicht → Hilfe**.
- **Einstellungen → Tastenkürzel** öffnet eine eigene Einstellungsseite als Tab. Befehl auswählen, im Aufnahmefeld eine Kombination drücken, **Zuweisen**, anschliessend **Für alle Fenster übernehmen**.
- Bestehende Ctrl-/Cmd-Befehle und zusätzliche Aktionen für Vergleich, Zoom und Split View lassen sich belegen. Alt- und Funktionstasten sind ebenfalls möglich. Doppelte Belegungen werden benannt; einzelne Belegungen lassen sich entfernen und die Standardbelegung wiederherstellen.
- Die Einstellungen werden lokal in `shortcuts.json` neben den übrigen Einstellungen gespeichert. Menü- und Tab-Kontextmenü-Hinweise verwenden die aktuelle Belegung. Nicht mehr zugewiesene frühere Standardkombinationen lösen im Texteditor nicht heimlich die alte Funktion aus.
- Die allgemeinen Tastenkürzel gelten für Editor-Befehle; Eingabefelder behalten ihre normale Zwischenablage-Bedienung. Systemweit reservierte Kombinationen können vom Betriebssystem abgefangen werden. Das Aufnahmefeld fängt eigene neue Kombinationen ab, ohne dabei den Editor-Befehl auszuführen.

## Vergleich schliessen und stoppen

- Die rechte Vergleichsansicht besitzt eine gestaltete Kopfzeile mit **Datei öffnen**, **Vergleichen**, **Stopp** und einem **X** zum Schliessen.
- **Stopp** beendet den aktiven Vergleich beziehungsweise entfernt die Vergleichsmarkierungen. Beide Texte bleiben erhalten. Laufende Berechnungsergebnisse werden nach einem Stopp ignoriert; die Hintergrundberechnung prüft Abbruchpunkte und behält das bestehende Zeitlimit von zwei Sekunden für den eigentlichen Diff-Schritt.
- **X** schliesst die Vergleichsansicht, stoppt den Vergleich und gibt den Platz dem linken Text zurück. Der rechte Text bleibt im Tab und Sitzungscache erhalten und erscheint beim erneuten Öffnen wieder.
- Späte Ergebnisse eines gestoppten, verschobenen oder erneut gestarteten Vergleichs überschreiben die aktuelle Ansicht nicht.

## Split View zusammenführen

- **Ansicht → Split View → Zusammenführen** und das Tab-Kontextmenü führen die beiden Seiten wieder zu einer Ansicht mit gemeinsamer Tab-Liste zusammen.
- Dokumente werden mit ihren bestehenden Editor-Widgets verschoben: Inhalte, Bilder, Undo-Zustand, Zoom, Dateipfade und Sync-Zuordnungen bleiben erhalten. Unbenutzte leere Start-Tabs und doppelte Einstellungs-Tabs werden entfernt. Laufende Vergleiche in verschobenen Tabs werden gestoppt.
- **Als zwei Fenster weiterführen** bleibt als eigene Aktion verfügbar. Damit sind Zusammenführen und Trennen in zwei Fenster ausdrücklich getrennt wählbar.

## Linux / Nobara: Anheften an die Taskleiste

- Fenster verwenden jetzt eine feste EditorTool-Kennung, passend zu `StartupWMClass` und dem Desktop-Dateinamen. RPM, DEB und Flatpak verwenden dieselbe Desktop-Identität für die Zuordnung in der Taskleiste.
- Portable Linux-Versionen legen beim Start einen Benutzer-Launcher und das Icon im XDG-Datenverzeichnis an, sofern keine passende Paketinstallation vorhanden ist. Ein eigener alter portabler Launcher wird bei Wechsel zur Paketinstallation entfernt. Fremde benutzerdefinierte Launcher werden nicht überschrieben.
- Nach dem Update alle EditorTool-Fenster beenden, neu starten und den laufenden Editor erneut anheften. Ein alter Eintrag mit der früheren Fensterkennung muss gegebenenfalls einmal entfernt werden. Unter Wayland verwendet die App weiterhin XWayland.

## Pakete und Hinweise

Windows Setup-EXE und portables ZIP; Linux Flatpak, RPM, DEB und portables TAR.GZ; macOS DMG und App-ZIP. Jeweils **x64 und ARM64**, dazu `SHA256SUMS.txt`.

Alle Funktionen aus V1.4.5 einschliesslich Sync-Countdown, stabilisierter Bildansichten und hybrider TLS-Verschlüsselung bleiben enthalten.

Auf Wunsch **keine automatisierten, interaktiven oder Installationstests**. Die Änderungen für Nobara wurden implementiert und für Linux paketiert, aber nicht in einer laufenden Desktop-Sitzung geprüft. Pakete ohne Entwicklerzertifikate; macOS mit Ad-hoc-Signatur ohne Notarisierung. Projektlizenz: **The Unlicense**; Drittanbieter behalten ihre Lizenzen.

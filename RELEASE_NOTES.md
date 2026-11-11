# EditorTool V1.4.5

## Ruhige Bildanzeige beim Sync

- Gewöhnliche Sync-Änderungen aktualisieren vorhandene Textblöcke und verwenden vorhandene Bild-Frames und dekodierte Bilddaten weiter. Der gesamte Dokumentbereich wird dafür nicht mehr abgerissen und neu erzeugt.
- Sichtbarer Bildanker, Scrollposition, Textcursor, Auswahl und Fokus werden beim Übernehmen einer Sync-Version erhalten. Ein notwendiger Neuaufbau, etwa bei neuen oder entfernten Bildern, erhält die Ansicht ebenfalls. Endgültiges Layout und Wiederherstellung erfolgen vor dem nächsten Zeichnen.
- Gemischte Text-/Bild-Dokumente verwenden eine feste Umbruchbreite für die Messung. Vorübergehende innere Scrollleisten verengen dadurch nicht mehr kurzzeitig den Text und verschieben die Bilder nach unten und wieder zurück. Die vertikale Bewegung übernimmt die äussere Dokument-Scrollleiste.
- Reine Bild- oder Grössenänderungen markieren das Dokument weiterhin als geändert und werden im Sitzungscache gespeichert. Text-Zoom und native Schriftmetriken aus V1.4.1 bleiben erhalten.

## Sync-Countdown in der Statusleiste

- Synchronisierte Dokumente zeigen unten **Sync in 5 s**, dann die verbleibenden Sekunden nach der letzten Eingabe. Jede weitere Änderung beginnt die Schreibpause erneut.
- Nach Ablauf erscheint der tatsächliche Abgleichstatus: ausstehende Freigabe, Übertragung/Bestätigung ausstehend, Warten auf einen offline Client, pausierter Sync oder Versionskonflikt.
- **Synchronisiert** wird erst angezeigt, wenn die aktuelle Revision von allen aktiven Teilnehmern durch deren Versionsmeldung beziehungsweise Empfangsbestätigung bestätigt wurde. Zuletzt bestätigte Stände mit inzwischen offline befindlichen Clients werden entsprechend bezeichnet.
- Der Countdown ist die lokale Schreibpause, keine Garantie für die Ankunft beim Empfänger nach exakt fünf Sekunden. Offline-Geräte, Freigaben und laufende Übertragungen können den anschliessenden Abgleich verzögern.
- Für den Countdown läuft nur während einer sichtbaren Schreibpause ein gemeinsamer Sekunden-Timer. Versteckte Tabs und der Tray erhalten keinen zusätzlichen dauerhaften Countdown-Timer.

## Neues Tab-Kontextmenü

- Rechtsklick auf einen Datei-Tab öffnet ein eigenes Menü mit abgerundeten Zeilen, Akzentfarbe, klaren Gruppen und Tastenkürzel-Hinweisen, passend zum übrigen Editor.
- Dateiaktionen, Senden/Sync und Tab-Aktionen sind gegliedert. Sprache und Split View besitzen eigene übersichtliche Unterseiten.
- Bedienung per Maus oder Pfeiltasten; Enter wählt, Escape beziehungsweise ein Klick ausserhalb schliesst das Menü. Einstellungs-Tabs zeigen nur die passenden Tab-Aktionen.

## Pakete und Hinweise

Windows Setup-EXE und portables ZIP; Linux Flatpak, RPM, DEB und portables TAR.GZ; macOS DMG und App-ZIP. Jeweils **x64 und ARM64**, dazu `SHA256SUMS.txt`.

Die hybride Verschlüsselung aus V1.4.1 bleibt erhalten: TLS 1.3, AES-256-GCM und X25519 + ML-KEM-768 für Senden/Sync, mit klassischen bestätigten Gerätesignaturen. V1.4.5 verwendet das gleiche Freigabe-/Sync-Protokoll wie V1.4.1. Für die neue Anzeige und Bildstabilisierung die betreffenden Geräte aktualisieren.

Auf Wunsch wurden **keine automatisierten, interaktiven oder Installationstests** ausgeführt. Pakete ohne Entwicklerzertifikate; macOS mit Ad-hoc-Signatur ohne Notarisierung. Lizenz: **The Unlicense** für Projektdateien; Drittanbieter behalten ihre Lizenzen.

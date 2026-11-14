# EditorTool V1.5.1

## Windows: lange Pfade und Speicherdiagnose

- Windows-Aufrufe für Zugriffsrechte und atomare Dateiersetzung verwenden normalisierte absolute Langpfade. Temporäre Dateien haben kurze, vom Dokumentnamen unabhängige Namen. Damit scheitert die Dateihistorie nicht mehr an einem intern erzeugten Pfad über der klassischen Windows-Pfadgrenze (Fehler 123).
- Diagnosemeldungen unterscheiden das Lesen der Datei, das Sichern der Historie, das Schreiben des Dokuments und das Setzen privater Zugriffsrechte. Logs enthalten Arbeitsschritt, Fehlerkategorie, Betriebssystem-Fehlernummer und Pfadlänge, keine Dokumentinhalte oder vollständigen privaten Dateipfade.
- Falls automatisches Speichern in V1.5.0 wegen des Fehlers pausiert wurde: nach dem Update beim betroffenen Dokument im Sync-Dialog „Sync-Änderungen in meine lokale Datei speichern“ wieder aktivieren. Bereits bewusst deaktiviertes Autosave wird nicht ungefragt eingeschaltet.

## Netzwerkadapter für Senden und Sync

- Unter **Einstellungen → Clients und Sync** die automatische Auswahl ausschalten und gewünschte lokale IPs anhaken. LAN, WireGuard und andere vom Betriebssystem bereitgestellte VPN-Adapter erscheinen mit Namen, IP und verfügbarer MAC-Adresse; virtuelle Adapter besitzen nicht immer eine MAC.
- Mehrere IPs sind wählbar. Empfangs-Ports binden nur an die gewählten Adressen; ausgehende Verbindungen werden an gewählte Quell-IP und Schnittstelle gebunden. Bei nicht verfügbarer Auswahl erfolgt kein stiller Rückfall auf andere Adapter. Die Funktion erstellt weder VPN-Tunnel noch Firewall-Regeln; das Betriebssystem muss eine passende Route besitzen.
- Bei nicht verfügbarem VPN bleibt die Konfiguration erreichbar und Senden/Sync pausiert. Adapter lassen sich manuell neu einlesen. Im Modus „Automatisch“ gelten wie bisher das Betriebssystem-Routing und die privaten Netzbeschränkungen.

## Clients, Status und Hinweise

- Einen Client auswählen, Name/IP/Port bearbeiten und **Änderungen speichern** wählen. Zertifikat, Fingerabdruck und bestehende Freigabe bleiben erhalten. Ein Gerät mit anderem Schlüssel muss weiterhin ausdrücklich freigegeben werden. Manuell gepflegte Namen und Endpunkte werden nicht von empfangenen Hello-Nachrichten überschrieben; automatisch entdeckte Geräte können ihren tatsächlichen Port weiterhin mitteilen.
- Beidseitige Gerätefreigabe bleibt erforderlich: Jede Seite entscheidet, welchem Gerät sie vertraut. Dokumentanfragen bleiben eine zusätzliche Zustimmung.
- **Connected** wird grün, **Not connected** rot dargestellt, auch bei der Auswahl von Send-/Sync-Empfängern.
- Nach einer Prüfrunde folgen normalerweise 10 Sekunden Pause, bei ausstehenden Sendungen 1 Sekunde. Verbindungsversuche und Übertragungen verlängern die Runde; dies ist kein garantiertes Zehn-Sekunden-Erkennungsintervall. Die letzte Prüfung wird angezeigt. **Verbindung prüfen** startet manuell eine Runde und prüft ausstehende Freigaben erneut.
- Beim Wechsel eines zuvor verbundenen Geräts zu nicht verbunden erscheint unten rechts im Editor ein nicht blockierender Hinweis. Mit **X** quittieren; Hinweise bleiben bis dahin sichtbar. Keine System-Popups und kein Fokuswechsel. Der Status beschreibt den letzten vollständigen Abgleich, keine dauerhaft offene TCP-Verbindung.

## Kommunikationsprotokoll

- **Log…** im Send-/Sync-Dialog oder **Kommunikationsprotokoll…** in den Client-Einstellungen öffnet einen Tab. Gerätefilter, Richtung und Textsuche grenzen die Einträge ein; die gefilterte Ansicht kann exportiert werden.
- Angezeigt werden TCP-/TLS-Verbindungsereignisse, Protokolltypen wie Hello, Offer, Get/Put, Annahmen/Ablehnungen, Übertragungsgrössen und Fehlerkategorien. Bytezahlen sind Anwendungsdaten, kein vollständiger Paketmitschnitt. Inhalte, Bilder und Schlüssel werden nicht aufgezeichnet.
- Dieses lokale Protokoll enthält zur Diagnose IP-Adressen und Geräte-Fingerabdrücke. Es wird getrennt vom allgemeinen Diagnoseexport gespeichert: höchstens 1500 Einträge im Viewer, zwei begrenzte Dateien unter `traffic`. Kein automatischer Upload.

## Pakete und Validierung

Windows Setup-EXE und portables ZIP; Linux Flatpak, RPM, DEB und portables TAR.GZ; macOS DMG und App-ZIP. Jeweils x64 und ARM64. Dazu SHA256SUMS.txt, update-metadata.json und update-metadata.sig mit derselben Ed25519-Vertrauensbasis wie V1.5.0.

Auf Wunsch nur Codeprüfung, native Release-Builds und Paketierung: keine automatisierten, interaktiven oder Installationstests und kein App-Start. Keine Windows-/Apple-Entwicklerzertifikate; macOS ad-hoc signiert, ohne Notarisierung. The Unlicense für eigenen Code, Drittanbieter-Lizenzen bleiben erhalten.

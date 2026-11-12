# EditorTool V1.5.0 – Release-Entwurf

Noch nicht veröffentlicht. Dieser Entwurf darf erst nach Einrichtung des dauerhaften Signierschlüssels und erfolgreichem Bau aller 16 Plattformpakete als Release veröffentlicht werden.

## Sicherheit

Unabhängig signierte Ed25519-Update-Metadaten binden Version und alle Pakete an den fest eingebauten öffentlichen Schlüssel. Der Updater akzeptiert ausschliesslich neuere Versionen und prüft die Integrität beim Download, vor Übergabe und nochmals im Installationshelfer. Windows-/Apple-Zertifikate sind davon unabhängig weiterhin nicht vorhanden. Nutzer bis V1.4.9 erhalten den neuen Vertrauensschlüssel erst durch ein manuell vertrautes Upgrade; die alte Updateprüfung kann nachträglich nicht verbessert werden.

Dateien werden vor dem Einlesen begrenzt, Bilder vor dem Decodieren geprüft. Grenzen: 64 MiB je Dokument, 32 MiB komprimiert je Bild, 40 Megapixel insgesamt, 32768 Pixel je Seite, 64 Bilder. Die Betriebssystem-Zwischenablage liefert möglicherweise bereits decodierte Daten; vor eigenen Kopien wird deren Grösse geprüft.

Sitzungen und Instanzdaten sind lokal restriktiv geschützt; Instanz-Tokens sind kryptografisch zufällig. Sync-Verbindungen laufen höchstens 45 Sekunden, mit begrenzter Parallelität und 240 eingehenden Verbindungen je Minute und Gegenstellen-IP. TLS 1.3, AES-256-GCM, hybrider X25519+ML-KEM-768-Schlüsselaustausch und ausdrückliche Freigaben bleiben erhalten.

## Speicher und Wiederherstellung

Unter Einstellungen den belegten Speicher ansehen und verwaiste Tabs, ältere Dateistände oder Update-Dateien getrennt bereinigen. Aktuelle und vorige Sitzung, geschlossene Fenster und Sync-/Geräteidentitäten bleiben erhalten. Löschung ist endgültig und verspricht kein physisches Überschreiben.

Vor dem Überschreiben einer lokalen Datei wird ihr vorheriger Inhalt gesichert: zehn Stände bzw. 256 MiB je Dateipfad. Datei → Ältere Dateistände öffnet den gewählten Stand als neuen Tab, ohne den aktuellen Inhalt zu ersetzen. Historie liegt unverschlüsselt im Benutzerprofil und ist gezielt löschbar.

## Freigaben und Flatpak

Die Berechtigungsübersicht zeigt Geräte, Fingerabdrücke, aktive Teilnehmer und ausstehende Sendungen. Gerätefreigabe und Dokumentzustimmung bleiben getrennt; beendeter Sync löscht keine fremden Kopien.

Flatpak verwendet Dateiauswahl-Portale für Öffnen/Speichern, Bilder und Diagnoseexport; die globale Home-Freigabe entfällt. Alte Dateipfade gegebenenfalls erneut auswählen. Für `.etdoc` muss die Dateiendung bereits im Portal-Speicherdialog stimmen. Host-Zugriff für Flatpak-Updates, Netzwerk, X11 und Tray bleiben erforderlich.

## Validierung und Veröffentlichung

Keine Tests, App-Starts oder Installationstests. Linux-x64-Codeprüfung und Release-Build sowie Packaging werden protokolliert. Die übrigen nativen Builds müssen über GitHub Actions erfolgen. Eine vollständige Veröffentlichung erfordert 16 Pakete, SHA256SUMS.txt und die beiden signierten Metadaten-Dateien.

# EditorTool V1.4

## Senden und Synchronisieren im internen Netzwerk

- **Einstellungen → Clients, Senden und Sync**: Geräte über interne IPv4/IPv6-Adresse und Port hinzufügen; Status **Connected / Not connected**. Auch geroutete interne Netze und VPN-Verbindungen sind möglich. Öffentliche Internet-Adressen werden abgelehnt. Standard: **TCP 51473**, lokal und je Client änderbar.
- Senden/Sync ist standardmässig ausgeschaltet. Erst beim Aktivieren wird der lokale Freigabe-Port geöffnet. Verbindungen verwenden verschlüsseltes gegenseitiges TLS und gespeicherte Geräteschlüssel. Beim ersten Verbinden die Fingerabdrücke auf beiden Geräten vergleichen und bestätigen.
- **Rechtsklick auf einen Tab → Senden an Client**: genau einen Client auswählen. Das Gegenstück nimmt die Anfrage an oder lehnt sie ab. Die Datei wird erst nach Annahme übertragen und als neuer Tab geöffnet; optional mit einer selbst gewählten lokalen Datei.
- **Rechtsklick auf einen Tab → Synchronisieren**: mehrere Clients auswählen. Jeder Empfänger entscheidet selbst. Teilnehmer der Gruppe werden in der Anfrage angezeigt und nach Annahme für diese Gruppe freigegeben.
- Änderungen werden nach **fünf Sekunden ohne weitere Eingabe** zur Synchronisierung bereitgestellt. Verbundene Geräte gleichen sich im Hintergrund ab; bei erneuter Erreichbarkeit werden ausstehende Versionen nachgeholt. Die Verbindungskontrolle erfolgt etwa alle zehn Sekunden, bei ausstehenden Freigaben häufiger. Übertragung und Verbindungsaufbau blockieren den Editor nicht.
- Geräte-Zähler und Inhaltsprüfsummen bestimmen die Versionsfolge; PC-Uhrzeiten spielen dabei keine Rolle. Gleichzeitige unterschiedliche Änderungen bleiben als getrennte Versionen erhalten. **Sync-Versionen / Konflikte** zeigt eine Vorschau und lässt eine Version auswählen. Die Entscheidung wird als neue gemeinsame Version verteilt. Nicht übernommene Inhalte bleiben im lokalen Versionsspeicher erhalten.
- Pro Dokument lässt sich **Sync-Änderungen in meine lokale Datei speichern** aktivieren. Andernfalls werden nur Tab und Sitzungscache aktualisiert. Wurde die Datei extern geändert, pausiert automatisches Speichern mit einem Hinweis; beide Fassungen bleiben erhalten und können manuell verglichen werden. Bilder benötigen das `.etdoc`-Format.
- **Beim Schliessen im Tray weiterlaufen** ist wählbar und standardmässig aus. Das Tray-Menü öffnet Editor/Clients oder beendet die App vollständig. Es wird kein Windows-/Linux-Systemdienst installiert. Ohne erreichbaren Linux-System-Tray bleibt das Fenster sichtbar; GNOME benötigt gegebenenfalls eine StatusNotifier-/AppIndicator-Erweiterung.
- Anfragen, bestätigte Empfangsdaten, Sync-Zuordnungen und ausstehende Versionen werden lokal gesichert. Änderungen während einer Trennung bleiben erhalten. Beim Beenden vor Ablauf der Schreibpause sichert der Sitzungscache den Entwurf; nach dem nächsten Start wird er zur Synchronisierung bereitgestellt.

## Einrichtung und Grenzen

Auf allen beteiligten Geräten V1.4 installieren, Senden/Sync aktivieren, die internen IPs/Ports hinzufügen und die Geräte beidseitig freigeben. Der eingestellte TCP-Port muss durch lokale Firewalls und zwischen den betreffenden Netzen erreichbar sein. EditorTool verändert keine Firewall-Regeln und richtet keine Portweiterleitung ein. Es gibt keine Cloud, keine Internet-Vermittlung und keine automatische Geräteerkennung. Der GitHub-Updater verwendet unabhängig davon weiterhin HTTPS ins Internet.

Text, Code und eingebettete Bilder werden zusammen übertragen. Maximal 64 MiB je Dokument, 32 Geräte inklusive des eigenen Geräts je Sync-Gruppe. Vergleichstexte auf der rechten Vergleichsseite und lokale Dateipfade werden nicht übertragen. Ein geänderter Teilnehmerkreis startet eine neue Gruppe mit neuen Anfragen; die bisherigen Kopien bleiben erhalten. Sync beenden pausiert diese lokale Kopie, ohne andere Teilnehmer zu löschen.

Der lokale Ordner `sharing` neben dem Einstellungsordner enthält die Geräteidentität, Freigaben und Inhaltsversionen. Er ist wie der Sitzungscache unverschlüsselt auf der lokalen Festplatte und gehört nicht ins Repository. Private Geräteschlüssel nicht an andere Personen weitergeben. Historische Inhalte werden in V1.4 nicht automatisch gelöscht; Speicherbelegung kann mit übertragenen Dokumenten wachsen.

## Installationspakete

Windows: Setup-EXE und portables ZIP. Linux: Flatpak, RPM, DEB und portables TAR.GZ. macOS: DMG und App-ZIP. Jeweils **x64 und ARM64**, dazu `SHA256SUMS.txt`.

Alle Funktionen aus V1.3 sind enthalten. Auf Wunsch wurden **keine automatisierten, interaktiven oder Installationstests** ausgeführt. Die Pakete sind ohne Entwicklerzertifikate veröffentlicht; macOS erhält eine Ad-hoc-Signatur ohne Notarisierung. Prüfsummen ersetzen keine Herausgebersignatur. Projektlizenz: **The Unlicense**; Drittanbieter behalten ihre Lizenzen.

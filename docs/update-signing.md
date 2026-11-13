# Update-Metadaten signieren

Der Ed25519-Schlüssel ist unabhängig von Windows-/Apple-Zertifikaten. Der private
Schlüssel gehört dauerhaft in das Repository-Actions-Secret
`UPDATE_SIGNING_KEY_B64` (Base64 eines PKCS#8-PEM-Schlüssels). Nur der rohe öffentliche
32-Byte-Schlüssel steht Base64-kodiert in `assets/update-public-key.base64`.
Eine leere Datei sperrt Update-Installation und Release-Signierung ausdrücklich.
V1.5.0 darf damit nicht veröffentlicht werden.

Vor der ersten Schlüsselerzeugung muss authentifizierter Zugriff zum Schreiben
und Bestätigen des dauerhaften Actions-Secrets funktionieren. Schlüssel nicht in
Git, Chat, Shell-Argumente, Artefakte oder Protokolle schreiben. Temporäre Dateien
brauchen einen privaten Ordner und 0600-Rechte. Erst nach bestätigter Speicherung
des Secrets darf die temporäre private Kopie entfernt werden. Ein Actions-Secret
kann später nicht ausgelesen werden; für Schlüsselwechsel bleibt die bisherige
Signiermöglichkeit nötig oder eine manuelle Migration erforderlich.

Der Release-Job signiert die exakten Bytes von `update-metadata.json` und prüft
vorher Tag, Version, alle 16 Paketnamen und den öffentlichen Schlüssel des Secrets.
Die Metadaten binden Version, Paketname, Länge, SHA-256 und ein Ablaufdatum (180 Tage). Die App merkt sich die höchste bereits authentifiziert angebotene Version. Abgelaufene Metadaten müssen mit demselben Schlüssel neu signiert und im Release ersetzt werden. `update-metadata.sig`
enthält die Base64-kodierte Ed25519-Signatur. Die App prüft Signatur und eine strikt
neuere Version vor dem Download, erneut vor der Übergabe und SHA-256 zusätzlich
im Installationshelfer nach dem Ende des alten Prozesses. Kein Rückfall auf
unsignierte Prüfsummen. Alte Clients bis 1.4.9 können den Vertrauensschlüssel erst
mit einem manuell vertrauten Upgrade erhalten; ihre alte Prüfsummenprüfung kann
nachträglich nicht gehärtet werden.

Die Signierdatei ist ein Packaging-Schritt, kein Test. Es werden keine Tests oder
App-/Installationstarts ausgeführt. Alle sechs nativen Builds und sämtliche
Installer müssen erfolgreich sein. Erst danach darf der Release veröffentlicht
werden. Der Schlüssel liegt wie vom Projektinhaber gewählt im Actions-Secret. Personen mit Schreibzugriff auf Workflows oder Verwaltung des Secrets gehören damit zur Vertrauensgrenze: ein kompromittierter Workflow kann auch diesen Signierschlüssel verwenden. Die Signatur schützt insbesondere gegen den blossen Austausch von Release-Dateien. Schlüsselwechsel erfordert eine bereits vertrauenswürdige App-Version.

Die Ersteinrichtung wurde abgeschlossen. Eine zusätzliche Wiederherstellungskopie ist ausserhalb des Repositorys im lokalen Windows-Benutzerprofil durch DPAPI verschlüsselt gespeichert. Sie ist an diesen Windows-Benutzer gebunden; keine private Kopie gehört in Git oder Artefakte.

Vor Ablauf kann unter Actions der manuelle Workflow **Renew signed update metadata**
mit dem bestehenden Release-Tag gestartet werden. Er lädt die 16 unveränderten
Pakete, prüft den passenden Schlüssel, setzt eine neue Laufzeit und ersetzt nur
die beiden Metadaten-Dateien. Während des kurzen Austauschs der zwei Dateien kann
eine Updateprüfung sicher ablehnen; danach kann sie erneut gestartet werden.

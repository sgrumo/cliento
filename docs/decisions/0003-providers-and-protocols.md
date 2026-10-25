# 0003: Providers and protocols

## Status

Accepted.

## Decision

| Provider | Read | Send | Sign-in |
| --- | --- | --- | --- |
| Gmail | IMAP + Gmail extensions | SMTP | OAuth 2.0 (XOAUTH2) |
| Microsoft 365, Outlook.com | Microsoft Graph | Microsoft Graph | OAuth 2.0 |
| Fastmail | IMAP (JMAP later) | SMTP | App password, OAuth later |
| iCloud | IMAP | SMTP | App-specific password |
| Other | IMAP | SMTP | Password or OAuth when offered |

## Rationale

- Gmail's IMAP extensions expose labels, thread ids and server-side search; that covers the
  feature set with one protocol stack shared with other IMAP servers.
- Microsoft is retiring Exchange Web Services for Exchange Online, so new code targets Graph.
- JMAP is cleaner for sync but only Fastmail offers it widely; it can come after IMAP.

## Consequences

- Two sync implementations to maintain: IMAP and Graph. Both implement the same engine ports.

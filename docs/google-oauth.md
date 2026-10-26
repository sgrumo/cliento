# Google OAuth setup

ClientMail signs in to Gmail with OAuth 2.0 and PKCE. Each developer uses their own Google Cloud
OAuth client; its id stays out of the repository.

## 1. Google Cloud project

1. Open the [Google Cloud console](https://console.cloud.google.com/) and create a project, for
   example "ClientMail dev".
2. Nothing needs enabling: IMAP with XOAUTH2 does not use the Gmail API.

## 2. Consent screen

1. In **Google Auth Platform**, configure the consent screen: user type **External**, app name
   "ClientMail", your address as support and developer contact.
2. Add the scopes `https://mail.google.com/`, `openid`, `email` and `profile`.
3. Leave the publishing status on **Testing** and add every Google account you will sign in with
   under **Test users** (at most 100).

While the app is in Testing, Google expires refresh tokens after 7 days. ClientMail then needs you
to sign in again, which replaces the token.

## 3. OAuth client

1. Create an OAuth client of type **iOS**.
2. Bundle ID: `dev.samuelemedici.clientmail`.
3. Copy the client id (`1234-abcd.apps.googleusercontent.com`). iOS clients have no client secret.

ClientMail redirects to the reversed client id, `com.googleusercontent.apps.1234-abcd:/oauth2redirect`.
Google accepts that scheme for iOS clients without registering it.

## 4. Configure the app

```bash
cp apps/macos/Config/GoogleOAuth.example.xcconfig apps/macos/Config/GoogleOAuth.xcconfig
```

Set `GOOGLE_CLIENT_ID` in the copy, then run `make app`. `GoogleOAuth.xcconfig` is gitignored.
Without it the app builds, and the Google sign-in button is hidden.

## 5. A test access token for `make imap-smoke`

`make imap-smoke` signs in to Gmail's IMAP server with a short-lived access token and lists your
folders, printing only how many folders have each role. Until the app's sync (step 1.8) can hand
out a token, take one from Google's OAuth 2.0 Playground:

1. Open the [OAuth 2.0 Playground](https://developers.google.com/oauthplayground/).
2. In step 1, enter the scope `https://mail.google.com/` and authorize it with your test account.
3. In step 2, exchange the code for tokens and copy the access token. It expires after an hour.

```bash
CLIENTMAIL_IMAP_USER=you@gmail.com CLIENTMAIL_IMAP_TOKEN=ya29... make imap-smoke
```

The token belongs to the Playground's own client, which is fine for this test: Gmail checks the
token's scope, not which app asked for it. Revoke the Playground's access afterwards under your
Google account's third-party access.


.PHONY: check core-lint core-test core-doc bridge swift-lint swift-format swift-test keychain-test imap-smoke app-project app

CORE := core
KIT := apps/macos/Packages/ClientMailKit
# Handwritten Swift only: generated bindings are excluded.
SWIFT_SOURCES := apps/macos/ClientMail $(KIT)/Package.swift \
	$(KIT)/Sources/MailDomain $(KIT)/Sources/CoreAdapter $(KIT)/Sources/AccountsFeature $(KIT)/Tests
SWIFT_FORMAT_CONFIG := --configuration apps/macos/.swift-format

check: core-lint core-test core-doc bridge swift-lint swift-test app

core-lint:
	cd $(CORE) && cargo fmt --all --check
	cd $(CORE) && cargo clippy --workspace --all-targets -- -D warnings

core-test:
	cd $(CORE) && cargo test --workspace

core-doc:
	cd $(CORE) && RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps

bridge:
	scripts/build-core-bridge.sh

swift-lint:
	xcrun swift-format lint $(SWIFT_FORMAT_CONFIG) --strict --recursive $(SWIFT_SOURCES)

swift-format:
	xcrun swift-format format $(SWIFT_FORMAT_CONFIG) --in-place --recursive $(SWIFT_SOURCES)

swift-test: bridge
	cd $(KIT) && swift test

# Opt-in: writes to and deletes from the real login Keychain under a unique test service.
keychain-test: bridge
	cd $(KIT) && CLIENTMAIL_KEYCHAIN_TESTS=1 swift test --filter SystemKeychainTests

# Opt-in: signs in to Gmail's IMAP server and lists folders. Needs CLIENTMAIL_IMAP_USER and
# CLIENTMAIL_IMAP_TOKEN (docs/google-oauth.md, section 5). Prints counts per role, never names.
imap-smoke:
	cd $(CORE) && cargo test -p clientmail-imap --test gmail_smoke -- --ignored --nocapture

app-project:
	cd apps/macos && xcodegen generate --quiet

app: bridge app-project
	xcodebuild -project apps/macos/ClientMail.xcodeproj -scheme ClientMail \
		-destination 'platform=macOS,arch=arm64' -derivedDataPath apps/macos/build \
		-quiet build

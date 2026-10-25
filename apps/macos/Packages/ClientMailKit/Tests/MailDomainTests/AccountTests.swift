import MailDomain
import Testing

struct AccountTests {
    @Test func titleUsesDisplayNameWhenSet() {
        let account = Account(id: "1", address: "ada@example.com", displayName: "Ada", provider: .gmail)

        #expect(account.title == "Ada")
    }

    @Test func titleFallsBackToAddress() {
        let account = Account(id: "1", address: "ada@example.com", displayName: nil, provider: .gmail)

        #expect(account.title == "ada@example.com")
    }
}

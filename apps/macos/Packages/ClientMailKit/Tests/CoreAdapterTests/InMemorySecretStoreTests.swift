import CoreAdapter
import Testing

struct InMemorySecretStoreTests {
    @Test func writeReadAndDeleteRoundTrip() throws {
        let store = InMemorySecretStore()

        try store.write(label: "a", secret: "token")
        let read = try store.read(label: "a")
        let deleted = try store.delete(label: "a")
        let deletedAgain = try store.delete(label: "a")

        #expect(read == "token")
        #expect(deleted)
        #expect(!deletedAgain)
        #expect(try store.read(label: "a") == nil)
    }
}

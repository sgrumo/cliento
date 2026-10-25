import MailDomain
import SwiftUI

/// One account: its name, address and provider.
struct AccountRow: View {
    let account: Account

    var body: some View {
        LabeledContent {
            Text(Strings.name(of: account.provider))
                .foregroundStyle(.secondary)
        } label: {
            Text(account.title)
            if account.displayName != nil {
                Text(account.address)
            }
        }
    }
}

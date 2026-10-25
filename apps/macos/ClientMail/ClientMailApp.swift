import SwiftUI

/// App entry point. Builds the dependencies once and hands them to the root view.
@main
struct ClientMailApp: App {
    @State private var startup = Startup.launch()

    var body: some Scene {
        WindowGroup {
            RootView(startup: startup)
        }
    }
}

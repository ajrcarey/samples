# Code samples

Some representative code samples from closed source personal projects, demonstrating code style and design.

* emitter.dart and observable.dart: Reactive programming for Dart using the classic Emitter/Observable pattern. Prior to the stablisation of Dart's async stream interface, the Emitter used its own internal microtask event loop; with the release of Dart streams, the code has become much simpler, as the event loop is now handled by simply wrapping a Dart stream. The public interface to the Reactive objects never changed, despite substantial internal refactoring in the move to streams.

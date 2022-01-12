# Code samples

Some representative code samples from closed source personal projects, demonstrating code style and design.

* emitter.dart and observable.dart: Reactive programming for Dart using the classic Emitter/Observable pattern. Prior to the stablisation of Dart's async stream interface, the Emitter used its own internal microtask event loop; with the release of Dart streams, the code has become much simpler, as the event loop is now handled by simply wrapping a Dart stream. The public interface to the Reactive objects never changed, despite substantial internal refactoring in the move to streams.
* filtered-index.tsx: An generic list filter component in Typescript/React with JSX. Allows interactive filtering and grouping irrespective of the source data format. Visible in action on the website of The London Festival of Contemporary Church Music at https://www.lfccm.com/whatson/composers/, https://www.lfccm.com/whatson/performers/, https://www.lfccm.com/whatson/works/, and https://www.lfccm.com/whatson/venues/.
* system.rs: An excerpt from a music notation processing system. This file defines the LayoutSystem struct, the implementation block of which takes sets of grid lines and notational blocks and lays them out on a two-dimensional surface according to linear constraints. The layout of music notation is thus decomposed into a linear constraint system; resolving the constraints in the linear constraint system results in a correctly laid out system of music notation.

## Licensing

Strictly GPL3 only. No warranty offered or implied. Purely intended as illustrative samples, not for real-world use.

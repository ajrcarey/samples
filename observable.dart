part of duet_stdlib;

class Observable<T> extends async.Stream<T> {
  final async.Stream<T> _underlying;

  Observable._(this._underlying);

  factory Observable.of(async.Stream<T> source) => Observable._(source);

  static Observable<E> merge<E>(final Iterable<Observable<E>> observables) {
    final merged = Emitter<E>();

    final count = Emitter<int>.initial(0)
      ..scopeTo(merged)
      ..observe((Option<int> v) {
        if (v.orElse(0) == observables.length) {
          merged.complete();
        }
      });

    for (var o in observables) {
      o
          .observe((Option<E> value) => value.match(merged.emit, merged.clear))
          .onDone(() => count.emit(count.orElse(0) + 1));
    }

    return merged;
  }

  T _value;

  T _defaultValue;

  T _effectiveValue;

  @override
  @Deprecated(
      'Where possible, subscribe to the stream using the observe() function rather than the listen() function. The observe() function is null-safe; listen() is not')
  async.StreamSubscription<T> listen(void Function(T event) onData,
      {Function onError, void Function() onDone, bool cancelOnError = false}) {
    if (onDone != null && isComplete) {
      onDone();
    }

    return _underlying.listen(onData,
        onError: onError, onDone: onDone, cancelOnError: cancelOnError);
  }

  async.StreamSubscription<T> observe(void Function(Option<T> v) onData,
          {void Function() onComplete}) =>
      _underlying.listen((T v) => onData(Maybe(v)), onDone: onComplete);

  bool _isComplete = false;

  bool get isComplete => _isComplete;

  void complete() {
    if (!_isComplete) {
      _isComplete = true;
      unscope();
    }
  }

  void completeIn(final int count) {
    var _count = 0;

    completeWhen((_) => ++_count > count);
  }

  void completeAt(final DateTime expiry) {
    if (!_isComplete) {
      Timer().at(expiry).then((_) => complete());
    }
  }

  void completeAfter(final Duration duration) {
    if (!_isComplete) {
      Timer().after(duration).then((_) => complete());
    }
  }

  // AJRC - 11/10/21 - ignore the lint here because we want to rebind the
  // function assigned to the variable.
  // ignore: prefer_function_declarations_over_variables
  Predicate<T> _completionPredicate = (_) => false;

  void completeWhen(final Predicate<T> test) =>
      _completionPredicate = (T v) => v != null && test(v);

  void completeWhenNot(final Predicate<T> test) =>
      completeWhen((T v) => v != null && !test(v));

  void completeOnceEmpty() => _completionPredicate = (T v) => v == null;

  void completeWith(final Observable<Object> that) =>
      that.observe((_) {}, onComplete: complete);

  Observable<Object> _scope;

  void scopeTo(final Observable<Object> that) => _scope = that;

  void unscope() => _scope = null;

  @override
  Observable<T> take(final int count) =>
      Observable<T>.of(_underlying.take(count));

  @override
  Observable<T> takeWhile(final Predicate<T> test) =>
      Observable<T>.of(_underlying.takeWhile(test));

  Observable<T> takeUntil(final Predicate<T> test) =>
      takeWhile((T value) => !test(value));

  @override
  Observable<T> skip(final int count) =>
      Observable<T>.of(_underlying.skip(count));

  @override
  Observable<T> skipWhile(final Predicate<T> test) =>
      Observable<T>.of(_underlying.skipWhile(test));

  Observable<T> skipUntil(final Predicate<T> test) =>
      skipWhile((T value) => !test(value));

  @override
  Observable<T> where(final Predicate<T> test) =>
      Observable<T>.of(_underlying.where((T v) => v != null && test(v)));

  Observable<T> whereNot(final Predicate<T> test) =>
      where((T value) => !test(value));

  Observable<T> wherePresent() {
    final e = Emitter<T>();

    observe((Option<T> value) => value.ifPresent(e.emit),
        onComplete: e.complete);

    return e;
  }

  Observable<T> whereDistinct() {
    final e = Emitter<T>();

    observe(
        (Option<T> value) => value.match(
            (T v) => e.match((T x) {
                  if (x != v) {
                    e.emit(v);
                  }
                }, () => e.emit(v)),
            () => e.ifPresent((_) => e.clear())),
        onComplete: e.complete);

    return e;
  }

  @override
  Future<T> reduce(final Combiner<T, T, T> combine) {
    final r = Pending<T>();

    T value;

    observe((Option<T> v) {
      v.ifPresent((T v) {
        if (value == null) {
          value = v;
        } else {
          value = combine(value, v);
        }
      });
    }, onComplete: () => r.complete(value));

    return r.future;
  }

  Observable<T> progressivelyReduce(final Combiner<T, T, T> combine) {
    final e = Emitter<T>();

    T value;

    observe((Option<T> v) {
      v.ifPresent((T v) {
        if (value == null) {
          value = v;
        } else {
          value = combine(value, v);
        }

        e.emitIfPresent(Maybe(value));
      });
    }, onComplete: e.complete);

    return e;
  }

  Future<Slice<T>> toSlice() {
    final c = async.Completer<Slice<T>>();

    final l = <T>[];

    observe((Option<T> v) => v.ifPresent(l.add))
        .onDone(() => c.complete(l.toSlice()));

    return c.future;
  }

  Future<Slice<Option<T>>> toOptionSlice() {
    final c = async.Completer<Slice<Option<T>>>();

    final l = <Option<T>>[];

    observe(l.add).onDone(() => c.complete(l.toSlice()));

    return c.future;
  }

  Observable<T> repeat(final int count) {
    final e = Emitter<T>();

    observe(
        (Option<T> value) => value.ifPresent((T value) {
              for (var i = 0; i < count; i++) {
                e.emit(value);
              }
            }),
        onComplete: e.complete);

    return e;
  }

  Observable<T> defaultIfEmpty(final T value) =>
      (clone() as Emitter<T>)..setDefault(value);

  Observable<T> delay(final Duration delay) {
    final e = Emitter<T>();

    observe((Option<T> value) =>
            Timer().after(delay).then((_) => value.match(e.emit, e.clear)))
        .onDone(() => Timer().after(delay).then((_) => e.complete()));

    return e;
  }

  Observable<Slice<T>> batch(final int count) {
    final e = Emitter<Slice<T>>();

    final l = <T>[];

    var i = 0;

    observe((Option<T> value) => value.ifPresent((T x) {
          i++;
          l.add(x);

          if (i == count) {
            e.emit(l.toList().toSlice()); // Take a copy of l with l.toList()...
            l.clear(); // ... so that clearing l doesn't effect the emitted slice
            i = 0;
          }
        })).onDone(() {
      if (l.isNotEmpty) {
        e.emit(l.toSlice());
      }

      e.complete();
    });

    return e;
  }

  Observable<Pair<T, T>> inPairs() => batch(2)
      .where((Iterable<T> l) => l.length == 2)
      .map((Iterable<T> l) => Pair<T, T>(l.first, l.last));

  Observable<Triplet<T, T, T>> inTriplets() =>
      batch(3).where((Slice<T> l) => l.length == 3).map((Slice<T> l) =>
          Triplet<T, T, T>(l.elementAt(0), l.elementAt(1), l.elementAt(2)));

  Observable<Quadruplet<T, T, T, T>> inQuadruplets() => batch(4)
      .where((Slice<T> l) => l.length == 4)
      .map((Slice<T> l) => Quadruplet<T, T, T, T>(
          l.elementAt(0), l.elementAt(1), l.elementAt(2), l.elementAt(3)));

  Observable<T> sample(final Duration every) {
    final e = Emitter<T>()..scopeTo(this);

    Timer().every(every)
      ..scopeTo(e)
      ..observe((_) => cond(e.emit, e.clear));

    return e;
  }

  Observable<Slice<T>> recent(final int count) {
    final e = Emitter<Slice<T>>();

    final q = corecoll.Queue<T>();

    observe(
        (Option<T> v) => v.ifPresent((T v) {
              q.add(v);

              while (q.length > count) {
                q.removeFirst();
              }

              e.emit(q.toList().toSlice()); // Take a copy of q with q.toList(),
              // so that calling q.add() and q.removeFirst() later
              // doesn't affect the emitted slice
            }),
        onComplete: e.complete);

    return e;
  }

  Observable<T> debounce(final Duration after) =>
      Observable<T>.of(_underlying.debounce(after));

  Observable<T> debounceLeading(final Duration after) => Observable<T>.of(
      _underlying.debounce(after, leading: true, trailing: false));

  Observable<T> throttle(final Duration every) =>
      Observable<T>.of(_underlying.throttle(every));

  Observable<T> mergeWith(final Observable<T> that) =>
      Observable.merge<T>(<Observable<T>>[this, that]);

  Observable<T> mergeWithAll(final Iterable<Observable<T>> those) =>
      Observable.merge<T>(<Observable<T>>[this, ...those]);

  Pair<Observable<T>, Observable<T>> split(final Predicate<T> test) =>
      Pair<Observable<T>, Observable<T>>(where(test), whereNot(test));

  void splitInto(
      final Emitter<T> a, final Emitter<T> b, final Predicate<T> test) {
    a.pullFrom(where(test));
    b.pullFrom(whereNot(test));
  }

  Observable<T> clone() {
    final e = Emitter<T>()
      .._value = _value
      .._defaultValue = _defaultValue
      .._effectiveValue = _effectiveValue;

    observe(e.emitOrClear, onComplete: e.complete);

    return e;
  }

  void copyTo(final Emitter<T> that) =>
      _effectiveValue != null ? that.emit(_effectiveValue) : that.clear();

  void copyToIfPresent(final Emitter<T> that) {
    if (isCurrentlyNotEmpty) {
      that.emit(_effectiveValue);
    }
  }

  void copyToIfDistinct(final Emitter<T> that) {
    if (_effectiveValue != null) {
      that.emitIfDistinct(_effectiveValue);
    }
  }

  void pushTo(final Emitter<T> that) {
    observe((Option<T> v) => v.ifPresent(that.emit));
  }

  void copyThenPushTo(final Emitter<T> that) => that.copyThenPullFrom(this);

  @override
  Future<void> forEach(final Consumer<T> action) {
    final c = async.Completer<void>();

    observe((Option<T> v) => v.ifPresent(action)).onDone(() => c.complete());

    return c.future;
  }

  Future<void> forEachIndexed(void Function(int index, T v) f) {
    var index = 0;

    final c = async.Completer<void>();

    observe((Option<T> v) => v.ifPresent((v) => f(index++, v)))
        .onDone(() => c.complete());

    return c.future;
  }

  @override
  Observable<E> map<E>(final Mapping<T, E> convert) {
    final e = Emitter<E>();

    observe((Option<T> v) => v.ifPresent((T v) => e.emit(convert(v))),
        onComplete: e.complete);

    ifPresent((T v) => e.emit(convert(v)));

    return e;
  }

  async.StreamSubscription<T> mapTo<E>(
          final Emitter<E> that, final Mapping<T, E> f) =>
      that.mapFrom(this, f);

  Observable<E> unwrapMap<E>(final Mapping<Option<T>, Option<E>> f) {
    final e = Emitter<E>();

    observe((Option<T> v) => f(v).match(e.emit, e.clear));
    f(Maybe(_effectiveValue)).match(e.emit, e.clear);

    return e;
  }

  Observable<E> mapIndexed<E>(E Function(int index, T v) f) {
    var index = 0;

    return wherePresent().map((T v) => f(index++, v));
  }

  void ifMissing(VoidFunction f) {
    if (isCurrentlyEmpty) {
      f();
    }
  }

  void ifPresent(final Consumer<T> f) {
    if (isCurrentlyNotEmpty) {
      f(_effectiveValue);
    }
  }

  void ifContains(final T value, VoidFunction f) {
    if (currentlyContains(value)) {
      f();
    }
  }

  bool get isCurrentlyEmpty => _effectiveValue == null;

  bool get isCurrentlyNotEmpty => _effectiveValue != null;

  bool currentlyContains(final T value) => _effectiveValue == value;

  void match(final Consumer<T> ifPresent, VoidFunction ifMissing) => this
    ..ifPresent(ifPresent)
    ..ifMissing(ifMissing);

  R cond<R>(final Mapping<T, R> ifPresent, final Supplier<R> ifMissing) =>
      _effectiveValue != null ? ifPresent(_effectiveValue) : ifMissing();

  Future<R> collectWith<A, R>(
      final CollectorInitializer<A> initializer,
      final CollectorAccumulator<T, A> accumulator,
      final CollectorFinalizer<A, R> finalizer) {
    var result = initializer();

    return forEach((T e) {
      result = accumulator(result, e);
    }).then((_) => finalizer(result));
  }

  Future<R> collect<A, R>(final Collector<T, A, R> collector) => collectWith(
      collector.initializer, collector.accumulator, collector.finalizer);

  Observable<R> progressivelyCollectWith<A, R>(
      final CollectorInitializer<A> initializer,
      final CollectorAccumulator<T, A> accumulator,
      final CollectorFinalizer<A, R> finalizer) {
    final o = Emitter<A>();

    observe((Option<T> v) =>
        v.ifPresent((T v) => o.emit(accumulator(o.orElseGet(initializer), v))));

    return o.map(finalizer);
  }

  Observable<R> progressivelyCollect<A, R>(
          final Collector<T, A, R> collector) =>
      progressivelyCollectWith(
          collector.initializer, collector.accumulator, collector.finalizer);

  Future<T> get next {
    final c = Pending<T>();

    observe((Option<T> value) => value.ifPresent(c.complete)).onDone(() {
      if (!c.isCompleted) {
        c.completeError(_e);
      }
    });

    return c.future;
  }

  T orElse(final T sentinel) => _effectiveValue ?? sentinel;

  T orElseGet(final Supplier<T> f) => _effectiveValue ?? f();

  T orElseThrow(final Exception e) {
    // ignore: prefer_if_null_operators
    return _effectiveValue != null ? _effectiveValue : throw e;
  }

  static final NullArgumentException _e = NullArgumentException();

  T orElsePanic() => orElseThrow(_e);

  T orElseCatch(final Mapping<NullArgumentException, T> f) =>
      _effectiveValue ?? f(_e);

  @override
  String toString() => 'Observable<$T = $_effectiveValue>';
}

extension AsyncStreamObservableExtension<T> on async.Stream<T> {
  Observable<T> asObservable() => Observable<T>.of(this);
}

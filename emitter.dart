part of duet_stdlib;

class Emitter<T> extends Observable<T> {
  final async.StreamController<T> _controller;

  Emitter._(async.StreamController<T> controller)
      : _controller = controller,
        super._(controller.stream);

  factory Emitter() => Emitter<T>._(async.StreamController<T>.broadcast());

  factory Emitter.initial(T initial) => Emitter<T>()..emit(initial);

  factory Emitter.fromIterable(final Iterable<T> values) =>
      Emitter<T>()..emitAll(values);

  factory Emitter.fromFuture(final Future<T> future) =>
      Emitter.fromFutures([future]);

  factory Emitter.fromFutures(final Iterable<Future<T>> futures) {
    final e = Emitter<T>();

    for (final future in futures) {
      future.then(e.emit);
    }

    return e;
  }

  factory Emitter.fromStream(final async.Stream<T> stream) =>
      Emitter.fromStreams([stream]);

  factory Emitter.fromStreams(final Iterable<async.Stream<T>> streams) {
    final e = Emitter<T>();

    for (final stream in streams) {
      stream.listen(e.emit, onDone: e.complete);
    }

    return e;
  }

  Observable<T> get observable => this;

  void _emit(T value) {
    if (!_isComplete) {
      _value = value;

      if (_value == null && _defaultValue != null) {
        _effectiveValue = _defaultValue;
      } else {
        _effectiveValue = _value;
      }

      if (_scope != null) {
        if (_scope._isComplete) {
          complete();
          return;
        }
      }

      if (_completionPredicate(value)) {
        complete();
        return;
      }

      _controller.add(_effectiveValue);
    }
  }

  void emit(final T value) => _emit(value);

  void emitAll(Iterable<T> values) => values.forEach(emit);

  void emitIfDistinct(final T value) {
    if (!currentlyContains(value)) {
      emit(value);
    }
  }

  void emitIfPresent(final Option<T> value) => value.ifPresent(emit);

  void emitOrClear(final Option<T> value) => value.match(emit, clear);

  void emitWhenReady(final Future<T> value) => value.then(emit);

  void clear() => _emit(null);

  void setDefault(final T value) {
    _defaultValue = value;

    if (_value == null) {
      _effectiveValue = _defaultValue;
      emit(null);
    }
  }

  void clearDefault() {
    _defaultValue = null;

    if (_value == null) {
      _effectiveValue = _value;
      emit(null);
    }
  }

  void emitDefault() => _emit(_defaultValue);

  @override
  void complete() {
    if (!_isComplete) {
      _controller.close();
      super.complete();
    }
  }

  void copyFrom(final Observable<T> that) =>
      emitOrClear(Maybe(that._effectiveValue));

  void copyFromIfPresent(final Observable<T> that) => that.ifPresent(emit);

  void copyFromIfDistinct(final Observable<T> that) {
    if (that._effectiveValue != null) {
      emitIfDistinct(that._effectiveValue);
    }
  }

  void copyThenPullFrom(final Observable<T> that) => this
    ..copyFrom(that)
    ..pullFrom(that);

  void pullFrom(final Observable<T> that) => that.pushTo(this);

  void pullDefaultFrom(final Observable<T> that) =>
      that.observe((Option<T> value) => value.match(setDefault, clearDefault));

  void copyDefaultFrom(final Observable<T> that) =>
      setDefault(that._effectiveValue);

  void copyDefaultFromIfPresent(final Observable<T> that) =>
      that.ifPresent(setDefault);

  async.StreamSubscription<E> mapFrom<E>(
          final Observable<E> that, final Mapping<E, T> f) =>
      that.observe((Option<E> v) => v.ifPresent((E v) => emit(f(v))));

  void synchronizeWith(final Emitter<T> that) =>
      mapWith(that, (T v) => v, (T v) => v);

  void mapWith<E>(final Emitter<E> that, final Mapping<E, T> mapFrom,
      final Mapping<T, E> mapTo) {
    that.observe(
        (Option<E> v) => v.ifPresent((E e) => emitIfDistinct(mapFrom(e))));

    observe(
        (Option<T> v) => v.ifPresent((T v) => that.emitIfDistinct(mapTo(v))));
  }

  void setAndPull(final Observable<T> that) => this
    ..copyFrom(that)
    ..pullFrom(that);

  void setAndPush(final Emitter<T> that) => that.setAndPull(this);

  @override
  String toString() => 'Emitter<$T = $_effectiveValue>';
}

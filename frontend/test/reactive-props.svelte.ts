export function reactiveProps<Props extends Record<string, unknown>>(
  initial: Props,
) {
  const props = $state(initial);
  return props;
}

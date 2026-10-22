import copy from '../copy';

export default function BrandMark() {
  return (
    <div
      aria-hidden="true"
      className="flex size-logo items-center justify-center rounded-logo bg-brand text-logo text-on-brand"
    >
      {copy.brand}
    </div>
  );
}

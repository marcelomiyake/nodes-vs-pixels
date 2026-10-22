import { signUpViaBackend, type SignUp } from './api';
import SuccessPanel from './components/SuccessPanel';
import SignUpForm from './SignUpForm';
import { useSignUpForm } from './useSignUpForm';

export default function App({ signUp = signUpViaBackend }: Readonly<{ signUp?: SignUp }>) {
  const form = useSignUpForm(signUp);
  return (
    <main className="flex min-h-dvh items-center justify-center px-page-x py-page-y sm:py-page-y-wide">
      <div className="w-full max-w-card rounded-card border border-line bg-surface p-card-pad shadow-card sm:p-card-pad-wide">
        {form.status === 'success' ? (
          <SuccessPanel name={form.values.fullName.trim()} />
        ) : (
          <SignUpForm form={form} />
        )}
      </div>
    </main>
  );
}

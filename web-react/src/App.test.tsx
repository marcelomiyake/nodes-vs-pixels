import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import type { SignUp, SignUpResult } from './api';
import App from './App';

const VALID = {
  'Full name': 'Ada Lovelace',
  Email: 'ada@example.com',
  Password: 'Analytical#Engine1',
  'Confirm password': 'Analytical#Engine1',
};

function setup(signUp: SignUp = vi.fn<SignUp>().mockResolvedValue({ ok: true })) {
  const user = userEvent.setup();
  render(<App signUp={signUp} />);
  return { user, signUp };
}

async function fillValidForm(user: ReturnType<typeof userEvent.setup>) {
  for (const [label, value] of Object.entries(VALID)) {
    await user.type(screen.getByLabelText(label), value);
  }
  await user.click(screen.getByLabelText(/I agree to the terms/));
}

describe('App', () => {
  it('renders the sign-up form with the password hint', () => {
    setup();
    expect(screen.getByRole('heading', { name: 'Create your account' })).toBeInTheDocument();
    expect(screen.getByRole('form', { name: 'Create your account' })).toBeInTheDocument();
    expect(screen.getByLabelText('Password')).toHaveAccessibleDescription(
      'Use upper and lowercase letters, a number and a symbol.',
    );
    expect(screen.getByRole('button', { name: 'Create account' })).toBeEnabled();
  });

  it('shows an error once a field is blurred', async () => {
    const { user } = setup();
    const name = screen.getByLabelText('Full name');
    await user.type(name, 'A');
    expect(name).not.toHaveAttribute('aria-invalid', 'true');

    await user.tab();

    expect(name).toHaveAttribute('aria-invalid', 'true');
    expect(name).toHaveAccessibleDescription('Name must be at least 2 characters.');
  });

  it('validates everything on submit and focuses the first invalid field', async () => {
    const { user, signUp } = setup();

    await user.click(screen.getByRole('button', { name: 'Create account' }));

    expect(signUp).not.toHaveBeenCalled();
    expect(screen.getByLabelText('Full name')).toHaveFocus();
    expect(screen.getByText('Enter your email address.')).toBeInTheDocument();
    expect(screen.getByLabelText(/I agree to the terms/)).toHaveAccessibleDescription(
      'Accept the terms to continue.',
    );
  });

  it('locks the values while submitting, then shows the success panel', async () => {
    let resolve: (result: SignUpResult) => void = () => undefined;
    const signUp = vi.fn<SignUp>(() => new Promise((done) => (resolve = done)));
    const { user } = setup(signUp);
    await fillValidForm(user);

    await user.click(screen.getByRole('button', { name: 'Create account' }));

    expect(screen.getByRole('button', { name: 'Creating account…' })).toBeDisabled();
    await user.type(screen.getByLabelText('Full name'), 'x');
    await user.click(screen.getByLabelText(/I agree to the terms/));
    expect(screen.getByLabelText('Full name')).toHaveValue('Ada Lovelace');
    expect(screen.getByLabelText(/I agree to the terms/)).toBeChecked();
    expect(signUp).toHaveBeenCalledWith({
      fullName: 'Ada Lovelace',
      email: 'ada@example.com',
      password: 'Analytical#Engine1',
      confirmPassword: 'Analytical#Engine1',
      acceptTerms: true,
    });
    resolve({ ok: true });
    const heading = await screen.findByRole('heading', { name: 'Account created' });
    await waitFor(() => {
      expect(heading).toHaveFocus();
    });
    expect(screen.getByText('Welcome aboard, Ada Lovelace. Your account is ready.')).toBeInTheDocument();
  });

  it('shows server violations until the field changes', async () => {
    const signUp = vi.fn<SignUp>().mockResolvedValue({
      ok: false,
      violations: [{ field: 'email', code: 'INVALID_FORMAT' }],
    });
    const { user } = setup(signUp);
    await fillValidForm(user);

    await user.click(screen.getByRole('button', { name: 'Create account' }));

    const email = screen.getByLabelText('Email');
    await waitFor(() => {
      expect(email).toHaveFocus();
    });
    expect(email).toHaveAccessibleDescription('Enter a valid email, like ada@example.com.');

    await user.type(email, 'm');

    expect(email).not.toHaveAttribute('aria-invalid', 'true');
  });

  it('reports an unusable server response as a failure', async () => {
    const { user } = setup(vi.fn<SignUp>().mockResolvedValue({ ok: false, violations: [] }));
    await fillValidForm(user);

    await user.click(screen.getByRole('button', { name: 'Create account' }));

    expect(await screen.findByRole('alert')).toHaveTextContent("We couldn't reach the server. Please try again.");
  });

  it('reports network failures and lets the user retry', async () => {
    const signUp = vi.fn<SignUp>().mockRejectedValueOnce(new Error('offline')).mockResolvedValueOnce({ ok: true });
    const { user } = setup(signUp);
    await fillValidForm(user);

    await user.click(screen.getByRole('button', { name: 'Create account' }));
    expect(await screen.findByRole('alert')).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Create account' }));
    expect(await screen.findByRole('heading', { name: 'Account created' })).toBeInTheDocument();
  });

  it('toggles the terms checkbox', async () => {
    const { user } = setup();
    const terms = screen.getByLabelText(/I agree to the terms/);

    await user.click(terms);
    expect(terms).toBeChecked();
    await user.click(terms);
    await user.tab();

    expect(terms).not.toBeChecked();
    expect(terms).toHaveAttribute('aria-invalid', 'true');
  });
});

import { useMemo } from 'react';
import { toast } from 'sonner';

export function useToastApi() {
  return useMemo(
    () => ({
      showToast(message, type = 'success') {
        if (type === 'success') {
          toast.success(message);
        } else {
          toast.error(message);
        }
      },
    }),
    []
  );
}

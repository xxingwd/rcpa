import { useOutletContext } from 'react-router-dom';
import ProvidersView from '../components/ProvidersView';

export default function ProvidersPage() {
  const { showToast } = useOutletContext();
  return <ProvidersView showToast={showToast} />;
}

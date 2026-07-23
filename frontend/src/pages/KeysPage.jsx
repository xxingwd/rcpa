import { useOutletContext } from 'react-router-dom';
import KeysView from '../components/KeysView';

export default function KeysPage() {
  const { showToast } = useOutletContext();
  return <KeysView showToast={showToast} />;
}

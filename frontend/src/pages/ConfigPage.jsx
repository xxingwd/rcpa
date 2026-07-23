import { useOutletContext } from 'react-router-dom';
import ConfigView from '../components/ConfigView';

export default function ConfigPage() {
  const { showToast } = useOutletContext();
  return (
    <div className="h-full min-h-0">
      <ConfigView showToast={showToast} />
    </div>
  );
}

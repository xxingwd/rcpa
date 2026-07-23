import { useOutletContext } from 'react-router-dom';
import LogsView from '../components/LogsView';

export default function LogsPage() {
  const { showToast } = useOutletContext();
  return (
    <div className="h-full min-h-0">
      <LogsView showToast={showToast} />
    </div>
  );
}
